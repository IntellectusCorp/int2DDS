"""
Verifies the XTypes TypeObject-advertisement path for generated Python types.

A type carrying generator-emitted ``_dds_type_info_fields`` metadata must create its
topic via ``int2dds_create_topic_with_type_info`` (advertising a conformant TypeObject),
still round-trip data, and still extract instance keys for keyed types.
"""

from dataclasses import dataclass, field
from typing import ClassVar

import pytest

from int2dds.cdr import CdrReader, CdrWriter, Extensibility
from int2dds.cdr.writer import CdrKeyWriter

try:
    from int2dds import DomainParticipant, WaitSet

    FFI_AVAILABLE = True
except ImportError:
    FFI_AVAILABLE = False

pytestmark = pytest.mark.skipif(not FFI_AVAILABLE, reason="FFI library not available")


@dataclass
class AdShape:
    """Flat keyed type with generator-style advertisement metadata (a generated ShapeType)."""

    _dds_type_name: ClassVar[str] = "AdShape"
    _extensibility: ClassVar[Extensibility] = Extensibility.APPENDABLE
    _has_key: ClassVar[bool] = True
    # (op, name, type_const, size, flags): INT2DDS_FIELD_* constants, MEMBER_KEY=1.
    _dds_type_info_fields: ClassVar[list] = [
        ("string", "color", 0, 128, 1),  # @key string<128>
        ("field", "x", 5, 0, 0),  # int32
        ("field", "shapesize", 5, 0, 0),  # int32
        ("seq", "payload", 7, 0, 0),  # sequence<uint8>
    ]

    color: str = "BLUE"
    x: int = 0
    shapesize: int = 30
    payload: list = field(default_factory=list)

    def _serialize_cdr(self, xcdr2: bool = True) -> bytes:
        w = CdrWriter(extensibility=self._extensibility, xcdr2=xcdr2)
        with w.dheader():
            w.write_string(self.color)
            w.write_i32(self.x)
            w.write_i32(self.shapesize)
            w.write_seq_header(len(self.payload))
            for b in self.payload:
                w.write_u8(b)
        return w.to_bytes()

    @classmethod
    def _deserialize_cdr(cls, data: bytes) -> "AdShape":
        r = CdrReader(data)
        _dsize, _dstart = r.read_dheader()
        color = r.read_string()
        x = r.read_i32()
        shapesize = r.read_i32()
        n = r.read_seq_header()
        payload = [r.read_u8() for _ in range(n)]
        r.read_dheader_end(_dsize, _dstart)
        return cls(color=color, x=x, shapesize=shapesize, payload=payload)

    def _serialize_key(self) -> bytes:
        w = CdrKeyWriter()
        w.write_string(self.color)
        return w.to_bytes()


def test_topic_created_via_type_info(domain_id: int):
    """A type with advertisement metadata creates its topic without error."""
    with DomainParticipant(domain_id=domain_id) as dp:
        topic = dp.create_topic("AdShapeTopic", AdShape)
        assert topic.type_name == "AdShape"
        topic.close()


def test_advertised_type_roundtrip(domain_id: int):
    """Advertising a TypeObject must not break intra-process matching / data flow."""
    from int2dds import DdsTimeout
    from int2dds.core.conditions import STATUS_DATA_AVAILABLE, STATUS_SUBSCRIPTION_MATCHED
    from int2dds.core.qos import DataReaderQos, DataWriterQos, Reliability

    with DomainParticipant(domain_id=domain_id) as dp:
        topic = dp.create_topic("AdShapeRoundtrip", AdShape)
        pub = dp.create_publisher()
        sub = dp.create_subscriber()
        writer = pub.create_datawriter(topic, qos=DataWriterQos(reliability=Reliability("RELIABLE")))
        reader = sub.create_datareader(topic, qos=DataReaderQos(reliability=Reliability("RELIABLE")))

        sc = reader.get_statuscondition()
        sc.set_enabled_statuses(STATUS_SUBSCRIPTION_MATCHED)
        ws = WaitSet()
        ws.attach(sc)
        deadline = 5.0
        while writer.matched_readers == 0 and deadline > 0:
            try:
                ws.wait(timeout=1.0)
            except DdsTimeout:
                pass
            deadline -= 1.0
        assert writer.matched_readers > 0, "advertised type failed to self-match"

        sc.set_enabled_statuses(STATUS_DATA_AVAILABLE)
        writer.write(AdShape(color="RED", x=7, shapesize=42, payload=[1, 2, 3]))
        try:
            ws.wait(timeout=5.0)
        except DdsTimeout:
            pass

        samples = reader.take()
        assert len(samples) > 0
        assert samples[0].data.color == "RED"
        assert samples[0].data.x == 7
        assert samples[0].data.payload == [1, 2, 3]


def test_advertised_keyed_instance_handle(domain_id: int):
    """A keyed advertised type still yields a non-NIL instance handle (key handling intact)."""
    with DomainParticipant(domain_id=domain_id) as dp:
        topic = dp.create_topic("AdShapeKeyed", AdShape)
        pub = dp.create_publisher()
        writer = pub.create_datawriter(topic)
        handle = writer.register_instance(AdShape(color="GREEN", x=1))
        assert len(handle) == 16
        assert handle != b"\x00" * 16
