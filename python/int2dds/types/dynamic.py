"""
Runtime dynamic types: TypeObject introspection, a TypeInfo builder and
DynamicData decoding of received samples by dotted/indexed field path.

Mirrors the C FFI surface in ffi/src/dynamic.rs and ffi/src/type_info.rs,
delegating all decoding to the core int2dds dynamic-type machinery.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from int2dds._ffi import ffi, lib
from int2dds.exceptions import check_ret

if TYPE_CHECKING:
    from int2dds.core.participant import DomainParticipant

# Field type constants.
FIELD_BOOL = 0
FIELD_BYTE = 1
FIELD_CHAR8 = 2
FIELD_INT8 = 3
FIELD_INT16 = 4
FIELD_INT32 = 5
FIELD_INT64 = 6
FIELD_UINT8 = 7
FIELD_UINT16 = 8
FIELD_UINT32 = 9
FIELD_UINT64 = 10
FIELD_FLOAT32 = 11
FIELD_FLOAT64 = 12
FIELD_STRING = 13
FIELD_CHAR16 = 14
FIELD_WSTRING = 15
FIELD_NESTED = 16
FIELD_SEQUENCE = 17
FIELD_ARRAY = 18
FIELD_MAP = 19

# Member flags.
MEMBER_KEY = 1 << 0
MEMBER_OPTIONAL = 1 << 1
MEMBER_MUST_UNDERSTAND = 1 << 2
MEMBER_EXTERNAL = 1 << 3


def _cstr(s: str) -> ffi.CData:
    return ffi.new("char[]", s.encode())


def _read_string(fn, *args) -> str:
    """Call a `(.., char *buf, uintptr_t cap, uintptr_t *out_len)` FFI getter,
    sizing the buffer from the reported required length."""
    out_len = ffi.new("uintptr_t*")
    probe = ffi.new("char[1]")
    ret = fn(*args, probe, 1, out_len)
    if ret == 0:
        return ffi.string(probe, out_len[0]).decode()
    need = out_len[0]
    buf = ffi.new(f"char[{need + 1}]")
    check_ret(fn(*args, buf, need + 1, out_len))
    return ffi.string(buf, out_len[0]).decode()


class TypeInfoBuilder:
    """Builds a TypeInfo (TypeIdentifier/TypeObject) for runtime type registration."""

    def __init__(self, type_name: str, extensibility: int = 1) -> None:
        out = ffi.new("Int2DdsTypeInfo **")
        check_ret(lib.int2dds_type_info_create(_cstr(type_name), int(extensibility), out))
        self._handle = out[0]

    def add_field(self, name: str, field_type: int, flags: int = 0) -> "TypeInfoBuilder":
        check_ret(lib.int2dds_type_info_add_field(self._handle, _cstr(name), field_type, flags))
        return self

    def add_sequence_field(
        self, name: str, element_type: int, bound: int = 0, flags: int = 0
    ) -> "TypeInfoBuilder":
        check_ret(
            lib.int2dds_type_info_add_sequence_field(
                self._handle, _cstr(name), element_type, bound, flags
            )
        )
        return self

    def add_array_field(
        self, name: str, element_type: int, array_size: int, flags: int = 0
    ) -> "TypeInfoBuilder":
        check_ret(
            lib.int2dds_type_info_add_array_field(
                self._handle, _cstr(name), element_type, array_size, flags
            )
        )
        return self

    def add_named_type_field(
        self, name: str, type_hash_name: str, flags: int = 0
    ) -> "TypeInfoBuilder":
        check_ret(
            lib.int2dds_type_info_add_named_type_field(
                self._handle, _cstr(name), _cstr(type_hash_name), flags
            )
        )
        return self

    def add_sequence_of_named_field(
        self, name: str, element_hash_name: str, bound: int = 0, flags: int = 0
    ) -> "TypeInfoBuilder":
        check_ret(
            lib.int2dds_type_info_add_sequence_of_named_field(
                self._handle, _cstr(name), _cstr(element_hash_name), bound, flags
            )
        )
        return self

    def add_array_of_named_field(
        self, name: str, element_hash_name: str, array_size: int, flags: int = 0
    ) -> "TypeInfoBuilder":
        check_ret(
            lib.int2dds_type_info_add_array_of_named_field(
                self._handle, _cstr(name), _cstr(element_hash_name), array_size, flags
            )
        )
        return self

    def to_type_object(self) -> "TypeObject":
        """Build a standalone TypeObject (for local introspection/decoding)."""
        out = ffi.new("Int2DdsTypeObject **")
        check_ret(lib.int2dds_type_info_to_type_object(self._handle, out))
        return TypeObject(out[0])

    def _take(self) -> ffi.CData:
        """Transfer ownership of the handle (consumed by create_topic_with_type_info)."""
        h = self._handle
        self._handle = None
        return h

    def close(self) -> None:
        if getattr(self, "_handle", None) is not None:
            lib.int2dds_type_info_destroy(self._handle)
            self._handle = None

    def __enter__(self) -> "TypeInfoBuilder":
        return self

    def __exit__(self, *exc) -> None:
        self.close()


class TypeObject:
    """A discovered TypeObject; supports struct member introspection."""

    def __init__(self, handle: ffi.CData) -> None:
        self._handle = handle

    @property
    def handle(self) -> ffi.CData:
        return self._handle

    @property
    def extensibility(self) -> int:
        out = ffi.new("int32_t*")
        check_ret(lib.int2dds_type_object_extensibility(self._handle, out))
        return out[0]

    def member_count(self) -> int:
        out = ffi.new("uint32_t*")
        check_ret(lib.int2dds_type_object_member_count(self._handle, out))
        return out[0]

    def member(self, index: int) -> dict:
        info = ffi.new("Int2DdsMemberInfo*")
        check_ret(lib.int2dds_type_object_member_info(self._handle, index, info))
        name = _read_string(lib.int2dds_type_object_member_name, self._handle, index)
        return {
            "name": name,
            "member_id": info.member_id,
            "kind": info.kind,
            "flags": info.flags,
        }

    def members(self) -> list:
        return [self.member(i) for i in range(self.member_count())]

    def find_member(self, name: str) -> int:
        out = ffi.new("uint32_t*")
        check_ret(lib.int2dds_type_object_find_member(self._handle, _cstr(name), out))
        return out[0]

    def close(self) -> None:
        if getattr(self, "_handle", None) is not None:
            lib.int2dds_type_object_destroy(self._handle)
            self._handle = None

    def __enter__(self) -> "TypeObject":
        return self

    def __exit__(self, *exc) -> None:
        self.close()


class DynamicData:
    """A decoded sample; read fields by dotted/indexed path (e.g. "pos.x", "tags[2]")."""

    def __init__(self, handle: ffi.CData) -> None:
        self._handle = handle

    def _get_scalar(self, fn, ctype: str, path: str):
        out = ffi.new(ctype)
        check_ret(fn(self._handle, _cstr(path), out))
        return out[0]

    def get_bool(self, path: str) -> bool:
        return bool(self._get_scalar(lib.int2dds_dynamic_data_get_bool, "bool*", path))

    def get_i8(self, path: str) -> int:
        return self._get_scalar(lib.int2dds_dynamic_data_get_i8, "int8_t*", path)

    def get_u8(self, path: str) -> int:
        return self._get_scalar(lib.int2dds_dynamic_data_get_u8, "uint8_t*", path)

    def get_i16(self, path: str) -> int:
        return self._get_scalar(lib.int2dds_dynamic_data_get_i16, "int16_t*", path)

    def get_u16(self, path: str) -> int:
        return self._get_scalar(lib.int2dds_dynamic_data_get_u16, "uint16_t*", path)

    def get_i32(self, path: str) -> int:
        return self._get_scalar(lib.int2dds_dynamic_data_get_i32, "int32_t*", path)

    def get_u32(self, path: str) -> int:
        return self._get_scalar(lib.int2dds_dynamic_data_get_u32, "uint32_t*", path)

    def get_i64(self, path: str) -> int:
        return self._get_scalar(lib.int2dds_dynamic_data_get_i64, "int64_t*", path)

    def get_u64(self, path: str) -> int:
        return self._get_scalar(lib.int2dds_dynamic_data_get_u64, "uint64_t*", path)

    def get_f32(self, path: str) -> float:
        return self._get_scalar(lib.int2dds_dynamic_data_get_f32, "float*", path)

    def get_f64(self, path: str) -> float:
        return self._get_scalar(lib.int2dds_dynamic_data_get_f64, "double*", path)

    def get_char8(self, path: str) -> int:
        return self._get_scalar(lib.int2dds_dynamic_data_get_char8, "uint8_t*", path)

    def get_string(self, path: str) -> str:
        return _read_string(lib.int2dds_dynamic_data_get_string, self._handle, _cstr(path))

    def get_len(self, path: str) -> int:
        out = ffi.new("uintptr_t*")
        check_ret(lib.int2dds_dynamic_data_get_len(self._handle, _cstr(path), out))
        return out[0]

    def get_member(self, path: str) -> "DynamicData":
        out = ffi.new("Int2DdsDynamicData **")
        check_ret(lib.int2dds_dynamic_data_get_member(self._handle, _cstr(path), out))
        return DynamicData(out[0])

    _SCALAR_BY_FIELD = {
        FIELD_BOOL: "get_bool",
        FIELD_BYTE: "get_u8",
        FIELD_CHAR8: "get_char8",
        FIELD_INT8: "get_i8",
        FIELD_INT16: "get_i16",
        FIELD_INT32: "get_i32",
        FIELD_INT64: "get_i64",
        FIELD_UINT8: "get_u8",
        FIELD_UINT16: "get_u16",
        FIELD_UINT32: "get_u32",
        FIELD_UINT64: "get_u64",
        FIELD_FLOAT32: "get_f32",
        FIELD_FLOAT64: "get_f64",
        FIELD_STRING: "get_string",
        FIELD_WSTRING: "get_string",
    }

    def get(self, path: str, field_type: int):
        """Read by a FIELD_* kind (e.g. from TypeObject member introspection)."""
        method = self._SCALAR_BY_FIELD.get(field_type)
        if method is None:
            raise ValueError(f"unsupported field type {field_type}")
        return getattr(self, method)(path)

    def close(self) -> None:
        if getattr(self, "_handle", None) is not None:
            lib.int2dds_dynamic_data_destroy(self._handle)
            self._handle = None

    def __enter__(self) -> "DynamicData":
        return self

    def __exit__(self, *exc) -> None:
        self.close()


def wait_for_type_object(
    participant: "DomainParticipant", topic_name: str, timeout_ms: int = -1
) -> tuple[TypeObject, str]:
    """Block until a publication for `topic_name` is discovered with a TypeObject."""
    obj_out = ffi.new("Int2DdsTypeObject **")
    name_buf = ffi.new("char[256]")
    out_len = ffi.new("uintptr_t*")
    check_ret(
        lib.int2dds_wait_for_type_object(
            participant.handle, _cstr(topic_name), timeout_ms, obj_out, name_buf, 256, out_len
        )
    )
    return TypeObject(obj_out[0]), ffi.string(name_buf, out_len[0]).decode()


def decode_sample(
    participant: "DomainParticipant", type_obj: TypeObject, data: bytes
) -> DynamicData:
    """Decode raw CDR `data` into a DynamicData using `type_obj` and the
    participant's type registry (for nested member resolution)."""
    out = ffi.new("Int2DdsDynamicData **")
    buf = ffi.from_buffer(data)
    check_ret(
        lib.int2dds_dynamic_data_from_sample(
            participant.handle, ffi.cast("const uint8_t*", buf), len(data), type_obj.handle, out
        )
    )
    return DynamicData(out[0])
