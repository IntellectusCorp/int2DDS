"""
Runtime dynamic types: TypeObject introspection, a TypeInfo builder and
DynamicData decoding of received samples by dotted/indexed field path.

Mirrors the C FFI surface in ffi/src/dynamic.rs and ffi/src/type_info.rs,
delegating all decoding to the core int2dds dynamic-type machinery.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from int2dds._ffi import CData, ffi, lib
from int2dds.exceptions import (
    INT2DDS_RET_INVALID_ARGUMENT,
    INT2DDS_RET_NO_DATA,
    INT2DDS_RET_NULL_POINTER,
    INT2DDS_RET_OK,
    check_ret,
)

if TYPE_CHECKING:
    from int2dds.core.participant import DomainParticipant
    from int2dds.core.publisher import Publisher
    from int2dds.core.subscriber import Subscriber

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

# DynamicValue kinds (returned by DynamicValue.kind()).
VALUE_KIND_BOOLEAN = 0
VALUE_KIND_INT8 = 1
VALUE_KIND_INT16 = 2
VALUE_KIND_INT32 = 3
VALUE_KIND_INT64 = 4
VALUE_KIND_UINT8 = 5
VALUE_KIND_UINT16 = 6
VALUE_KIND_UINT32 = 7
VALUE_KIND_UINT64 = 8
VALUE_KIND_FLOAT32 = 9
VALUE_KIND_FLOAT64 = 10
VALUE_KIND_CHAR8 = 11
VALUE_KIND_BYTE = 12
VALUE_KIND_STRING = 13
VALUE_KIND_WSTRING = 14
VALUE_KIND_ENUM = 15
VALUE_KIND_UNION = 16
VALUE_KIND_BITMASK = 17
VALUE_KIND_BITSET = 18
VALUE_KIND_STRUCT = 19
VALUE_KIND_SEQUENCE = 20
VALUE_KIND_ARRAY = 21
VALUE_KIND_MAP = 22
VALUE_KIND_OPTIONAL = 23
VALUE_KIND_NULL = 24


def _cstr(s: str) -> CData:
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

    def _take(self) -> CData:
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

    def __init__(self, handle: CData) -> None:
        self._handle = handle

    @property
    def handle(self) -> CData:
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

    def __init__(self, handle: CData) -> None:
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

    # --- Writable setters (build a sample to publish) ---

    def _set_scalar(self, fn, field: str, value) -> "DynamicData":
        check_ret(fn(self._handle, _cstr(field), value))
        return self

    def set_bool(self, field: str, value: bool) -> "DynamicData":
        return self._set_scalar(lib.int2dds_dynamic_data_set_bool, field, bool(value))

    def set_i8(self, field: str, value: int) -> "DynamicData":
        return self._set_scalar(lib.int2dds_dynamic_data_set_i8, field, value)

    def set_u8(self, field: str, value: int) -> "DynamicData":
        return self._set_scalar(lib.int2dds_dynamic_data_set_u8, field, value)

    def set_i16(self, field: str, value: int) -> "DynamicData":
        return self._set_scalar(lib.int2dds_dynamic_data_set_i16, field, value)

    def set_u16(self, field: str, value: int) -> "DynamicData":
        return self._set_scalar(lib.int2dds_dynamic_data_set_u16, field, value)

    def set_i32(self, field: str, value: int) -> "DynamicData":
        return self._set_scalar(lib.int2dds_dynamic_data_set_i32, field, value)

    def set_u32(self, field: str, value: int) -> "DynamicData":
        return self._set_scalar(lib.int2dds_dynamic_data_set_u32, field, value)

    def set_i64(self, field: str, value: int) -> "DynamicData":
        return self._set_scalar(lib.int2dds_dynamic_data_set_i64, field, value)

    def set_u64(self, field: str, value: int) -> "DynamicData":
        return self._set_scalar(lib.int2dds_dynamic_data_set_u64, field, value)

    def set_f32(self, field: str, value: float) -> "DynamicData":
        return self._set_scalar(lib.int2dds_dynamic_data_set_f32, field, value)

    def set_f64(self, field: str, value: float) -> "DynamicData":
        return self._set_scalar(lib.int2dds_dynamic_data_set_f64, field, value)

    def set_char8(self, field: str, value: int | str) -> "DynamicData":
        byte = ord(value) if isinstance(value, str) else value
        return self._set_scalar(lib.int2dds_dynamic_data_set_char8, field, byte)

    def set_string(self, field: str, value: str) -> "DynamicData":
        check_ret(lib.int2dds_dynamic_data_set_string(self._handle, _cstr(field), _cstr(value)))
        return self

    def set_value(self, field: str, value: "DynamicValue") -> "DynamicData":
        """Set `field` from a (possibly nested) DynamicValue tree. The value is
        consumed on success."""
        ret = lib.int2dds_dynamic_data_set_value(self._handle, _cstr(field), value._handle)
        if ret not in (INT2DDS_RET_NULL_POINTER, INT2DDS_RET_INVALID_ARGUMENT):
            value._handle = None
        check_ret(ret)
        return self

    def get_value(self, path: str) -> "DynamicValue":
        """Clone the value at a dotted/indexed path into a new DynamicValue tree."""
        out = ffi.new("Int2DdsDynamicValue **")
        check_ret(lib.int2dds_dynamic_data_get_value(self._handle, _cstr(path), out))
        return DynamicValue(out[0])

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


class DynamicValue:
    """A (possibly nested) value tree mirroring the core ``DynamicValue``.

    Build write payloads with the constructors and ``push``/``map_insert``;
    inspect read-back values with ``kind`` and the ``as_*``/``element`` getters.
    Ownership: a value handed to ``push``/``map_insert``/``union``/``set_value``
    is consumed on success and must not be reused.
    """

    def __init__(self, handle: CData) -> None:
        self._handle = handle

    @property
    def handle(self) -> CData:
        return self._handle

    @classmethod
    def _scalar(cls, fn, value) -> "DynamicValue":
        out = ffi.new("Int2DdsDynamicValue **")
        check_ret(fn(value, out))
        return cls(out[0])

    @classmethod
    def boolean(cls, v: bool) -> "DynamicValue":
        return cls._scalar(lib.int2dds_dynamic_value_bool, bool(v))

    @classmethod
    def i8(cls, v: int) -> "DynamicValue":
        return cls._scalar(lib.int2dds_dynamic_value_i8, v)

    @classmethod
    def i16(cls, v: int) -> "DynamicValue":
        return cls._scalar(lib.int2dds_dynamic_value_i16, v)

    @classmethod
    def i32(cls, v: int) -> "DynamicValue":
        return cls._scalar(lib.int2dds_dynamic_value_i32, v)

    @classmethod
    def i64(cls, v: int) -> "DynamicValue":
        return cls._scalar(lib.int2dds_dynamic_value_i64, v)

    @classmethod
    def u8(cls, v: int) -> "DynamicValue":
        return cls._scalar(lib.int2dds_dynamic_value_u8, v)

    @classmethod
    def u16(cls, v: int) -> "DynamicValue":
        return cls._scalar(lib.int2dds_dynamic_value_u16, v)

    @classmethod
    def u32(cls, v: int) -> "DynamicValue":
        return cls._scalar(lib.int2dds_dynamic_value_u32, v)

    @classmethod
    def u64(cls, v: int) -> "DynamicValue":
        return cls._scalar(lib.int2dds_dynamic_value_u64, v)

    @classmethod
    def f32(cls, v: float) -> "DynamicValue":
        return cls._scalar(lib.int2dds_dynamic_value_f32, v)

    @classmethod
    def f64(cls, v: float) -> "DynamicValue":
        return cls._scalar(lib.int2dds_dynamic_value_f64, v)

    @classmethod
    def byte(cls, v: int) -> "DynamicValue":
        return cls._scalar(lib.int2dds_dynamic_value_byte, v)

    @classmethod
    def bitmask(cls, v: int) -> "DynamicValue":
        return cls._scalar(lib.int2dds_dynamic_value_bitmask, v)

    @classmethod
    def bitset(cls, v: int) -> "DynamicValue":
        return cls._scalar(lib.int2dds_dynamic_value_bitset, v)

    @classmethod
    def char8(cls, v: int | str) -> "DynamicValue":
        byte = ord(v) if isinstance(v, str) else v
        return cls._scalar(lib.int2dds_dynamic_value_char8, byte)

    @classmethod
    def string(cls, v: str) -> "DynamicValue":
        out = ffi.new("Int2DdsDynamicValue **")
        check_ret(lib.int2dds_dynamic_value_string(_cstr(v), out))
        return cls(out[0])

    @classmethod
    def wstring(cls, v: str) -> "DynamicValue":
        out = ffi.new("Int2DdsDynamicValue **")
        check_ret(lib.int2dds_dynamic_value_wstring(_cstr(v), out))
        return cls(out[0])

    @classmethod
    def enum(cls, name: str, value: int) -> "DynamicValue":
        out = ffi.new("Int2DdsDynamicValue **")
        check_ret(lib.int2dds_dynamic_value_enum(_cstr(name), value, out))
        return cls(out[0])

    @classmethod
    def struct(cls, data: "DynamicData") -> "DynamicValue":
        out = ffi.new("Int2DdsDynamicValue **")
        check_ret(lib.int2dds_dynamic_value_struct(data._handle, out))
        return cls(out[0])

    @classmethod
    def sequence(cls) -> "DynamicValue":
        out = ffi.new("Int2DdsDynamicValue **")
        check_ret(lib.int2dds_dynamic_value_sequence(out))
        return cls(out[0])

    @classmethod
    def array(cls) -> "DynamicValue":
        out = ffi.new("Int2DdsDynamicValue **")
        check_ret(lib.int2dds_dynamic_value_array(out))
        return cls(out[0])

    @classmethod
    def map(cls) -> "DynamicValue":
        out = ffi.new("Int2DdsDynamicValue **")
        check_ret(lib.int2dds_dynamic_value_map(out))
        return cls(out[0])

    @classmethod
    def union(cls, discriminator: "DynamicValue", value: "DynamicValue") -> "DynamicValue":
        out = ffi.new("Int2DdsDynamicValue **")
        ret = lib.int2dds_dynamic_value_union(discriminator._handle, value._handle, out)
        if ret != INT2DDS_RET_NULL_POINTER:
            discriminator._handle = None
            value._handle = None
        check_ret(ret)
        return cls(out[0])

    def push(self, element: "DynamicValue") -> "DynamicValue":
        """Append to a sequence/array value. Consumes `element` on success."""
        ret = lib.int2dds_dynamic_value_push(self._handle, element._handle)
        if ret == INT2DDS_RET_OK:
            element._handle = None
        check_ret(ret)
        return self

    def map_insert(self, key: "DynamicValue", value: "DynamicValue") -> "DynamicValue":
        """Insert a key/value pair into a map value. Consumes both on success."""
        ret = lib.int2dds_dynamic_value_map_insert(self._handle, key._handle, value._handle)
        if ret == INT2DDS_RET_OK:
            key._handle = None
            value._handle = None
        check_ret(ret)
        return self

    def kind(self) -> int:
        out = ffi.new("int32_t*")
        check_ret(lib.int2dds_dynamic_value_kind(self._handle, out))
        return out[0]

    def _as_scalar(self, fn, ctype: str):
        out = ffi.new(ctype)
        check_ret(fn(self._handle, out))
        return out[0]

    def as_bool(self) -> bool:
        return bool(self._as_scalar(lib.int2dds_dynamic_value_as_bool, "bool*"))

    def as_i8(self) -> int:
        return self._as_scalar(lib.int2dds_dynamic_value_as_i8, "int8_t*")

    def as_i16(self) -> int:
        return self._as_scalar(lib.int2dds_dynamic_value_as_i16, "int16_t*")

    def as_i32(self) -> int:
        return self._as_scalar(lib.int2dds_dynamic_value_as_i32, "int32_t*")

    def as_i64(self) -> int:
        return self._as_scalar(lib.int2dds_dynamic_value_as_i64, "int64_t*")

    def as_u8(self) -> int:
        return self._as_scalar(lib.int2dds_dynamic_value_as_u8, "uint8_t*")

    def as_u16(self) -> int:
        return self._as_scalar(lib.int2dds_dynamic_value_as_u16, "uint16_t*")

    def as_u32(self) -> int:
        return self._as_scalar(lib.int2dds_dynamic_value_as_u32, "uint32_t*")

    def as_u64(self) -> int:
        return self._as_scalar(lib.int2dds_dynamic_value_as_u64, "uint64_t*")

    def as_f32(self) -> float:
        return self._as_scalar(lib.int2dds_dynamic_value_as_f32, "float*")

    def as_f64(self) -> float:
        return self._as_scalar(lib.int2dds_dynamic_value_as_f64, "double*")

    def as_char8(self) -> int:
        return self._as_scalar(lib.int2dds_dynamic_value_as_char8, "uint8_t*")

    def as_string(self) -> str:
        return _read_string(lib.int2dds_dynamic_value_as_string, self._handle)

    def as_enum(self) -> tuple[str, int]:
        out_value = ffi.new("int32_t*")
        out_len = ffi.new("uintptr_t*")
        probe = ffi.new("char[1]")
        ret = lib.int2dds_dynamic_value_as_enum(self._handle, probe, 1, out_len, out_value)
        if ret == INT2DDS_RET_OK:
            return ffi.string(probe, out_len[0]).decode(), out_value[0]
        need = out_len[0]
        buf = ffi.new(f"char[{need + 1}]")
        check_ret(lib.int2dds_dynamic_value_as_enum(self._handle, buf, need + 1, out_len, out_value))
        return ffi.string(buf, out_len[0]).decode(), out_value[0]

    def as_bitmask(self) -> int:
        return self._as_scalar(lib.int2dds_dynamic_value_as_bitmask, "uint64_t*")

    def as_bitset(self) -> int:
        return self._as_scalar(lib.int2dds_dynamic_value_as_bitset, "uint64_t*")

    def len(self) -> int:
        out = ffi.new("uintptr_t*")
        check_ret(lib.int2dds_dynamic_value_len(self._handle, out))
        return out[0]

    def __len__(self) -> int:
        return self.len()

    def _child(self, fn, index: int) -> "DynamicValue":
        out = ffi.new("Int2DdsDynamicValue **")
        check_ret(fn(self._handle, index, out))
        return DynamicValue(out[0])

    def element(self, index: int) -> "DynamicValue":
        return self._child(lib.int2dds_dynamic_value_element, index)

    def map_key(self, index: int) -> "DynamicValue":
        return self._child(lib.int2dds_dynamic_value_map_key, index)

    def map_value(self, index: int) -> "DynamicValue":
        return self._child(lib.int2dds_dynamic_value_map_value, index)

    def as_struct(self) -> "DynamicData":
        out = ffi.new("Int2DdsDynamicData **")
        check_ret(lib.int2dds_dynamic_value_as_struct(self._handle, out))
        return DynamicData(out[0])

    def union_discriminator(self) -> "DynamicValue":
        out = ffi.new("Int2DdsDynamicValue **")
        check_ret(lib.int2dds_dynamic_value_union_discriminator(self._handle, out))
        return DynamicValue(out[0])

    def union_value(self) -> "DynamicValue":
        out = ffi.new("Int2DdsDynamicValue **")
        check_ret(lib.int2dds_dynamic_value_union_value(self._handle, out))
        return DynamicValue(out[0])

    def close(self) -> None:
        if getattr(self, "_handle", None) is not None:
            lib.int2dds_dynamic_value_destroy(self._handle)
            self._handle = None

    def __enter__(self) -> "DynamicValue":
        return self

    def __exit__(self, *exc) -> None:
        self.close()


class DynamicTypeSupport:
    """A type's runtime support object; creates writable DynamicData instances and
    backs dynamic topics/endpoints. Obtained from :class:`XmlTypeRegistry`."""

    def __init__(self, handle: CData) -> None:
        self._handle = handle

    @property
    def handle(self) -> CData:
        return self._handle

    def create_data(self) -> DynamicData:
        out = ffi.new("Int2DdsDynamicData **")
        check_ret(lib.int2dds_dynamic_data_create(self._handle, out))
        return DynamicData(out[0])

    def close(self) -> None:
        if getattr(self, "_handle", None) is not None:
            lib.int2dds_dynamic_type_support_destroy(self._handle)
            self._handle = None

    def __enter__(self) -> "DynamicTypeSupport":
        return self

    def __exit__(self, *exc) -> None:
        self.close()


class DynamicTopic:
    """A topic backed by a :class:`DynamicTypeSupport` (no compile-time type)."""

    def __init__(self, handle: CData) -> None:
        self._handle = handle

    @property
    def handle(self) -> CData:
        return self._handle

    def close(self) -> None:
        if getattr(self, "_handle", None) is not None:
            lib.int2dds_delete_topic(self._handle)
            self._handle = None

    def __enter__(self) -> "DynamicTopic":
        return self

    def __exit__(self, *exc) -> None:
        self.close()


class DynamicDataWriter:
    """Publishes :class:`DynamicData` samples on a :class:`DynamicTopic`."""

    def __init__(self, handle: CData) -> None:
        self._handle = handle

    def write(self, data: DynamicData) -> None:
        check_ret(lib.int2dds_dynamic_writer_write(self._handle, data._handle))

    def publication_matched_count(self) -> int:
        out = ffi.new("int32_t*")
        check_ret(lib.int2dds_dynamic_writer_publication_matched_count(self._handle, out))
        return out[0]

    def close(self) -> None:
        if getattr(self, "_handle", None) is not None:
            lib.int2dds_dynamic_writer_destroy(self._handle)
            self._handle = None

    def __enter__(self) -> "DynamicDataWriter":
        return self

    def __exit__(self, *exc) -> None:
        self.close()


class DynamicDataReader:
    """Receives :class:`DynamicData` samples from a :class:`DynamicTopic`."""

    def __init__(self, handle: CData) -> None:
        self._handle = handle

    def take(self) -> DynamicData | None:
        """Take the next available sample, or ``None`` when none is ready."""
        out = ffi.new("Int2DdsDynamicData **")
        ret = lib.int2dds_dynamic_reader_take(self._handle, out, ffi.NULL)
        if ret == INT2DDS_RET_NO_DATA:
            return None
        check_ret(ret)
        return DynamicData(out[0])

    def subscription_matched_count(self) -> int:
        out = ffi.new("int32_t*")
        check_ret(lib.int2dds_dynamic_reader_subscription_matched_count(self._handle, out))
        return out[0]

    def close(self) -> None:
        if getattr(self, "_handle", None) is not None:
            lib.int2dds_dynamic_reader_destroy(self._handle)
            self._handle = None

    def __enter__(self) -> "DynamicDataReader":
        return self

    def __exit__(self, *exc) -> None:
        self.close()


def create_topic_dynamic(
    participant: "DomainParticipant", topic_name: str, support: DynamicTypeSupport
) -> DynamicTopic:
    out = ffi.new("Int2DdsTopic **")
    check_ret(
        lib.int2dds_create_topic_dynamic(
            participant.handle, _cstr(topic_name), support.handle, ffi.NULL, out
        )
    )
    return DynamicTopic(out[0])


def create_datawriter_dynamic(
    publisher: "Publisher", topic: DynamicTopic, support: DynamicTypeSupport
) -> DynamicDataWriter:
    out = ffi.new("Int2DdsDynamicDataWriter **")
    check_ret(
        lib.int2dds_create_datawriter_dynamic(
            publisher._handle, topic.handle, support.handle, ffi.NULL, out
        )
    )
    return DynamicDataWriter(out[0])


def create_datareader_dynamic(
    subscriber: "Subscriber", topic: DynamicTopic, support: DynamicTypeSupport
) -> DynamicDataReader:
    out = ffi.new("Int2DdsDynamicDataReader **")
    check_ret(
        lib.int2dds_create_datareader_dynamic(
            subscriber._handle, topic.handle, support.handle, ffi.NULL, out
        )
    )
    return DynamicDataReader(out[0])
