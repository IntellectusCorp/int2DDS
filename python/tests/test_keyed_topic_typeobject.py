"""
Regression tests for #334/#336 raw-path keyed instance handling.

Covers the two halves of commit 6576b310 ("Repair raw-path XCDR2 keyed dispose;
reject keyed topics without a TypeObject"):

  1. A keyed topic created without any TypeObject / field metadata is rejected at
     creation (DdsUnsupported) instead of silently yielding NIL instance handles.
  2. A keyed topic advertised with a full TypeObject (generator-emitted
     ``_dds_type_info_fields``) derives a stable non-NIL instance handle and
     supports the full register -> write -> dispose -> unregister lifecycle,
     including under XCDR2 (the representation whose keyed dispose was repaired).
  3. An unkeyed type keeps a NIL handle.

DRAFT: the instance ops require the native FFI library and a live participant.
"""

from dataclasses import dataclass
from typing import ClassVar

import pytest

from int2dds.cdr import CdrWriter, Extensibility

try:
    from int2dds import DomainParticipant
    from int2dds.core.qos import DataRepresentation, DataWriterQos
    from int2dds.exceptions import DdsUnsupported

    FFI_AVAILABLE = True
except ImportError:
    FFI_AVAILABLE = False

pytestmark = pytest.mark.skipif(not FFI_AVAILABLE, reason="FFI library not available")

HANDLE_NIL = b"\x00" * 16


@dataclass
class KeyedShape:
    """Keyed type advertised with a full TypeObject (color is the @key)."""

    _dds_type_name: ClassVar[str] = "KeyedShape"
    _extensibility: ClassVar[Extensibility] = Extensibility.APPENDABLE
    _has_key: ClassVar[bool] = True
    # (op, name, type_const, size, flags): INT2DDS_FIELD_* constants, MEMBER_KEY=1.
    _dds_type_info_fields: ClassVar[list] = [
        ("string", "color", 0, 128, 1),  # @key string<128>
        ("field", "x", 5, 0, 0),  # int32
    ]

    color: str = "BLUE"
    x: int = 0

    def _serialize_cdr(self, xcdr2: bool = False) -> bytes:
        w = CdrWriter(extensibility=self._extensibility, xcdr2=xcdr2)
        if xcdr2:
            with w.dheader():
                w.write_string(self.color)
                w.write_i32(self.x)
        else:
            w.write_string(self.color)
            w.write_i32(self.x)
        return w.to_bytes()


@dataclass
class NameOnlyKeyed:
    """Keyed type with NO TypeObject / field metadata -> rejected at create time."""

    _dds_type_name: ClassVar[str] = "NameOnlyKeyed"
    _extensibility: ClassVar[Extensibility] = Extensibility.FINAL
    _has_key: ClassVar[bool] = True

    id: int = 0

    def _serialize_cdr(self, xcdr2: bool = False) -> bytes:
        w = CdrWriter(extensibility=self._extensibility, xcdr2=xcdr2)
        w.write_u32(self.id)
        return w.to_bytes()


@dataclass
class Unkeyed:
    """Non-keyed type -> instance handle stays NIL."""

    _dds_type_name: ClassVar[str] = "UnkeyedMsg"
    _extensibility: ClassVar[Extensibility] = Extensibility.FINAL
    _has_key: ClassVar[bool] = False

    value: int = 0

    def _serialize_cdr(self, xcdr2: bool = False) -> bytes:
        w = CdrWriter(extensibility=self._extensibility, xcdr2=xcdr2)
        w.write_i32(self.value)
        return w.to_bytes()


def test_name_only_keyed_topic_rejected(domain_id: int):
    """A keyed topic without a TypeObject is rejected, not silently NIL-handled."""
    with DomainParticipant(domain_id=domain_id) as dp:
        with pytest.raises(DdsUnsupported):
            dp.create_topic("NameOnlyKeyedTopic", NameOnlyKeyed)


def test_typeobject_keyed_register_then_lookup_match(domain_id: int):
    """A TypeObject-advertised keyed type derives a stable, key-only instance handle."""
    with DomainParticipant(domain_id=domain_id) as dp:
        topic = dp.create_topic("KeyedShapeLookup", KeyedShape)
        writer = dp.create_publisher().create_datawriter(topic)

        handle = writer.register_instance(KeyedShape(color="GREEN", x=1))
        assert len(handle) == 16
        assert handle != HANDLE_NIL

        # Same key (color), different value -> same instance handle.
        assert writer.lookup_instance(KeyedShape(color="GREEN", x=99)) == handle
        # A different key is a different (unregistered) instance -> NIL, not this handle.
        assert writer.lookup_instance(KeyedShape(color="RED", x=1)) != handle


def test_unkeyed_type_register_returns_nil(domain_id: int):
    """An unkeyed type yields the NIL handle."""
    with DomainParticipant(domain_id=domain_id) as dp:
        topic = dp.create_topic("UnkeyedTopic", Unkeyed)
        writer = dp.create_publisher().create_datawriter(topic)
        assert writer.register_instance(Unkeyed(value=7)) == HANDLE_NIL


@pytest.mark.parametrize("repr_kind", ["XCDR1", "XCDR2"])
def test_typeobject_keyed_dispose(domain_id: int, repr_kind: str):
    """register -> write -> dispose succeeds on the raw keyed path (the repaired path)."""
    with DomainParticipant(domain_id=domain_id) as dp:
        topic = dp.create_topic(f"KeyedShapeDispose{repr_kind}", KeyedShape)
        qos = DataWriterQos(data_representation=DataRepresentation(kind=repr_kind))
        writer = dp.create_publisher().create_datawriter(topic, qos=qos)

        sample = KeyedShape(color="BLUE", x=5)
        handle = writer.register_instance(sample)
        assert handle != HANDLE_NIL

        writer.write(sample)
        writer.dispose(sample, handle)


@pytest.mark.parametrize("repr_kind", ["XCDR1", "XCDR2"])
def test_typeobject_keyed_unregister(domain_id: int, repr_kind: str):
    """register -> write -> unregister succeeds on the raw keyed path."""
    with DomainParticipant(domain_id=domain_id) as dp:
        topic = dp.create_topic(f"KeyedShapeUnregister{repr_kind}", KeyedShape)
        qos = DataWriterQos(data_representation=DataRepresentation(kind=repr_kind))
        writer = dp.create_publisher().create_datawriter(topic, qos=qos)

        sample = KeyedShape(color="BLUE", x=5)
        handle = writer.register_instance(sample)
        assert handle != HANDLE_NIL

        writer.write(sample)
        writer.unregister_instance(sample, handle)
