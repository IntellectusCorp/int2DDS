"""ValueFrame codec: flat pack/unpack between generated types and the kernel.

The frame is one contiguous little-endian buffer -- a fixed slot region whose
offsets are computed from the type once, then an arena holding strings and
sequence elements referenced by ``(offset, length)`` slots. The kernel
(``dds/src/xtypes/frame_codec.rs``) computes the same layout from the topic's
TypeObject; both sides hash their canonical layout string (FNV-1a 64) and the
hashes are compared through ``int2dds_topic_frame_info`` at topic creation.
Any divergence disables the frame path for that topic instead of producing
wrong wire bytes.

Coverage mirrors the kernel: fixed-width scalars, enums, utf-8 strings,
arrays/sequences of scalars or enums, and nested structs flattened inline.
``FrameCodec.build`` returns ``None`` for anything else and the legacy
per-field CDR codec stays in charge.
"""

from __future__ import annotations

import dataclasses
import struct
from operator import attrgetter

# INT2DDS_FIELD_* constant -> (canonical tag, slot width, struct format char).
# CHAR8 (2) is intentionally absent: the kernel represents it but the Python
# packer does not (yet), so such types keep the legacy codec.
_SCALARS = {
    0: ("bool", 1, "?"),
    1: ("byte", 1, "B"),
    3: ("i8", 1, "b"),
    4: ("i16", 2, "h"),
    5: ("i32", 4, "i"),
    6: ("i64", 8, "q"),
    7: ("u8", 1, "B"),
    8: ("u16", 2, "H"),
    9: ("u32", 4, "I"),
    10: ("u64", 8, "Q"),
    11: ("f32", 4, "f"),
    12: ("f64", 8, "d"),
}

_ENUM_TAG = ("enum", 4, "i")
_REF_SLOT = struct.Struct("<II")


def _fnv1a64(data: bytes) -> int:
    h = 0xCBF29CE484222325
    for b in data:
        h ^= b
        h = (h * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


class _Unsupported(Exception):
    """The type has a shape the frame does not represent."""


def _reshape(flat: list, dims: tuple):
    """Rebuild the nested-list object shape from row-major flat values."""
    for dim in reversed(dims[1:]):
        flat = [flat[i : i + dim] for i in range(0, len(flat), dim)]
    return flat


def _enum_class(cls) -> bool:
    return getattr(cls, "_dds_enum_info", None) is not None


def _elem_of(op: str, type_const, nested_cls=None):
    """Resolve a sequence/array element to (tag, width, char, converter)."""
    if op in ("seq", "arr", "arr_nd"):
        if type_const not in _SCALARS:
            raise _Unsupported
        tag, width, ch = _SCALARS[type_const]
        return tag, width, ch, None
    # seq_nested / arr_nested / arr_nested_nd: only enum elements are scalar.
    if nested_cls is not None and _enum_class(nested_cls):
        tag, width, ch = _ENUM_TAG
        return tag, width, ch, nested_cls
    raise _Unsupported


class _ClassSpec:
    __slots__ = ("cls", "steps")

    def __init__(self, cls, steps):
        self.cls = cls
        self.steps = steps


def _walk_class(type_class, prefix: str, cursor: int, canon: list):
    """Compile one class level; returns (_ClassSpec, cursor after this level)."""
    meta = getattr(type_class, "_dds_type_info_fields", None)
    if meta is None:
        raise _Unsupported
    try:
        dc_fields = dataclasses.fields(type_class)
    except TypeError:
        raise _Unsupported from None
    if len(dc_fields) != len(meta):
        raise _Unsupported

    steps = []
    run_attrs: list[str] = []
    run_chars: list[str] = []
    run_convs: list = []
    run_offset = 0

    def flush_run():
        nonlocal run_attrs, run_chars, run_convs
        if not run_attrs:
            return
        st = struct.Struct("<" + "".join(run_chars))
        getter = attrgetter(*run_attrs)
        single = len(run_attrs) == 1
        convs = tuple(run_convs) if any(c is not None for c in run_convs) else None
        steps.append(("run", st, getter, single, run_offset, convs))
        run_attrs, run_chars, run_convs = [], [], []

    for (op, field_name, type_const, size, _flags), dc_field in zip(meta, dc_fields):
        attr = dc_field.name
        path = f"{prefix}.{field_name}" if prefix else field_name

        if op == "field":
            if type_const not in _SCALARS:
                raise _Unsupported
            tag, width, ch = _SCALARS[type_const]
            canon.append(f"{path}:{tag}:{cursor};")
            if not run_attrs:
                run_offset = cursor
            run_attrs.append(attr)
            run_chars.append(ch)
            run_convs.append(None)
            cursor += width
        elif op == "nested" and _enum_class(type_const):
            tag, width, ch = _ENUM_TAG
            canon.append(f"{path}:{tag}:{cursor};")
            if not run_attrs:
                run_offset = cursor
            run_attrs.append(attr)
            run_chars.append(ch)
            run_convs.append(type_const)
            cursor += width
        elif op == "string":
            flush_run()
            canon.append(f"{path}:str:{cursor};")
            steps.append(("string", attr, cursor))
            cursor += 8
        elif op in ("seq", "seq_nested"):
            flush_run()
            nested_cls = type_const if op == "seq_nested" else None
            tag, width, ch, conv = _elem_of(op, type_const, nested_cls)
            canon.append(f"{path}:seq[{tag}]:{cursor};")
            steps.append(("seq", attr, cursor, ch, conv))
            cursor += 8
        elif op in ("arr", "arr_nd", "arr_nested", "arr_nested_nd"):
            flush_run()
            nested_cls = type_const if op in ("arr_nested", "arr_nested_nd") else None
            tag, width, ch, conv = _elem_of(op, type_const, nested_cls)
            if op in ("arr_nd", "arr_nested_nd"):
                # Multi-dimensional: nested lists on the object, flat in the frame.
                dims = tuple(int(d) for d in size)
                count = 1
                for dim in dims:
                    count *= dim
            else:
                dims = None
                count = int(size)
            canon.append(f"{path}:arr[{tag};{count}]:{cursor};")
            steps.append(("arr", attr, cursor, count, ch, conv, dims))
            cursor += width * count
        elif op == "nested":
            flush_run()
            sub, cursor = _walk_class(type_const, path, cursor, canon)
            steps.append(("nested", attr, sub))
        else:
            raise _Unsupported

    flush_run()
    return _ClassSpec(type_class, steps), cursor


def _pack_into(spec: _ClassSpec, obj, buf: bytearray) -> None:
    for step in spec.steps:
        kind = step[0]
        if kind == "run":
            _, st, getter, single, offset, _convs = step
            values = (getter(obj),) if single else getter(obj)
            st.pack_into(buf, offset, *values)
        elif kind == "string":
            _, attr, offset = step
            data = getattr(obj, attr).encode()
            _REF_SLOT.pack_into(buf, offset, len(buf), len(data))
            buf += data
        elif kind == "seq":
            _, attr, offset, ch, _conv = step
            items = getattr(obj, attr)
            _REF_SLOT.pack_into(buf, offset, len(buf), len(items))
            buf += struct.pack(f"<{len(items)}{ch}", *items)
        elif kind == "arr":
            _, attr, offset, count, ch, _conv, dims = step
            items = getattr(obj, attr)
            if dims is not None:
                for _ in range(len(dims) - 1):
                    items = [x for sub in items for x in sub]
            if len(items) != count:
                raise ValueError(
                    f"array field '{attr}' has {len(items)} elements, type says {count}"
                )
            struct.pack_into(f"<{count}{ch}", buf, offset, *items)
        else:  # nested
            _, attr, sub = step
            _pack_into(sub, getattr(obj, attr), buf)


def _unpack_class(spec: _ClassSpec, buf):
    args = []
    for step in spec.steps:
        kind = step[0]
        if kind == "run":
            _, st, _getter, _single, offset, convs = step
            values = st.unpack_from(buf, offset)
            if convs is None:
                args.extend(values)
            else:
                args.extend(
                    v if conv is None else conv(v) for v, conv in zip(values, convs)
                )
        elif kind == "string":
            _, _attr, offset = step
            off, length = _REF_SLOT.unpack_from(buf, offset)
            args.append(bytes(buf[off : off + length]).decode())
        elif kind == "seq":
            _, _attr, offset, ch, conv = step
            off, count = _REF_SLOT.unpack_from(buf, offset)
            values = struct.unpack_from(f"<{count}{ch}", buf, off)
            args.append([conv(v) for v in values] if conv else list(values))
        elif kind == "arr":
            _, _attr, offset, count, ch, conv, dims = step
            values = struct.unpack_from(f"<{count}{ch}", buf, offset)
            flat = [conv(v) for v in values] if conv else list(values)
            args.append(_reshape(flat, dims) if dims is not None else flat)
        else:  # nested
            _, _attr, sub = step
            args.append(_unpack_class(sub, buf))
    return spec.cls(*args)


class FrameCodec:
    """Compiled frame layout and pack/unpack plan for one generated type."""

    __slots__ = ("fixed_size", "schema_hash", "_spec")

    def __init__(self, fixed_size: int, schema_hash: int, spec: _ClassSpec):
        self.fixed_size = fixed_size
        self.schema_hash = schema_hash
        self._spec = spec

    @classmethod
    def build(cls, type_class) -> "FrameCodec | None":
        """Compile a codec, or None when the type is not frame-representable."""
        try:
            canon: list = []
            spec, fixed = _walk_class(type_class, "", 0, canon)
        except _Unsupported:
            return None
        text = f"frame-v0;fixed={fixed};" + "".join(canon)
        return cls(fixed, _fnv1a64(text.encode()), spec)

    def pack(self, sample) -> bytearray:
        buf = bytearray(self.fixed_size)
        _pack_into(self._spec, sample, buf)
        return buf

    def unpack(self, frame):
        return _unpack_class(self._spec, frame)
