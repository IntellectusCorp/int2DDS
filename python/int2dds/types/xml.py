"""
Runtime types defined in XML — the same workflow as the Rust ``XmlTypeRegistry``
and the C ``int2dds_xml_type_registry_*`` API. Load an XML type-definition file,
then build a :class:`DynamicTypeSupport` for any declared type and publish/subscribe
it through the dynamic FFI without any compile-time IDL.

    reg = XmlTypeRegistry.from_file("sensor_data.xml")
    support = reg.get_type_support("SensorData")
    topic = participant.create_topic_dynamic("SensorTopic", support)
    writer = publisher.create_datawriter_dynamic(topic, support)
"""

from __future__ import annotations

from int2dds._ffi import ffi, lib
from int2dds.exceptions import check_ret
from int2dds.types.dynamic import DynamicTypeSupport, TypeObject, _cstr, _read_string


class XmlTypeRegistry:
    """Holds types parsed from one or more XML files, keyed by fully-qualified name."""

    def __init__(self, handle: ffi.CData | None = None) -> None:
        if handle is None:
            out = ffi.new("Int2DdsXmlTypeRegistry **")
            check_ret(lib.int2dds_xml_type_registry_create(out))
            handle = out[0]
        self._handle = handle

    @classmethod
    def from_file(cls, path: str) -> "XmlTypeRegistry":
        """Create a registry and load `path` into it in one step."""
        out = ffi.new("Int2DdsXmlTypeRegistry **")
        check_ret(lib.int2dds_xml_type_registry_from_file(_cstr(path), out))
        return cls(out[0])

    def load_file(self, path: str) -> "XmlTypeRegistry":
        """Load additional types from an XML file."""
        check_ret(lib.int2dds_xml_type_registry_load_file(self._handle, _cstr(path)))
        return self

    def load_str(self, xml: str) -> "XmlTypeRegistry":
        """Load additional types from an in-memory XML string."""
        check_ret(lib.int2dds_xml_type_registry_load_str(self._handle, _cstr(xml)))
        return self

    def get_type_support(self, name: str) -> DynamicTypeSupport:
        """Build a dynamic type support (with its full dependency closure)."""
        out = ffi.new("Int2DdsDynamicTypeSupport **")
        check_ret(lib.int2dds_xml_type_registry_get_type_support(self._handle, _cstr(name), out))
        return DynamicTypeSupport(out[0])

    def get_type_object(self, name: str) -> TypeObject:
        """Return a loaded type's top-level TypeObject for introspection."""
        out = ffi.new("Int2DdsTypeObject **")
        check_ret(lib.int2dds_xml_type_registry_get_type_object(self._handle, _cstr(name), out))
        return TypeObject(out[0])

    def type_count(self) -> int:
        out = ffi.new("uintptr_t*")
        check_ret(lib.int2dds_xml_type_registry_type_count(self._handle, out))
        return out[0]

    def type_name(self, index: int) -> str:
        return _read_string(lib.int2dds_xml_type_registry_type_name, self._handle, index)

    def type_names(self) -> list[str]:
        return [self.type_name(i) for i in range(self.type_count())]

    def close(self) -> None:
        if getattr(self, "_handle", None) is not None:
            lib.int2dds_xml_type_registry_destroy(self._handle)
            self._handle = None

    def __enter__(self) -> "XmlTypeRegistry":
        return self

    def __exit__(self, *exc) -> None:
        self.close()
