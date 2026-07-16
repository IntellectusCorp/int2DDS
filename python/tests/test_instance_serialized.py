"""E2E tests for #333 A(2): serialized read_instance / take_instance.

Uses a keyed type with >=2 live instances (a keyless type makes the instance
filter vacuous). Verifies take of one instance returns only that instance's
samples AND leaves the other instance takeable (filter-before-remove), plus the
nil-handle error path matching the typed read/take_instance.
"""

import os
import sys
import time
from dataclasses import dataclass
from typing import ClassVar

import pytest

sys.path.insert(0, os.path.dirname(__file__))

from int2dds.cdr import CdrReader, CdrWriter, Extensibility
from int2dds.cdr.writer import CdrKeyWriter
from int2dds.core.participant import DomainParticipant
from int2dds.core.qos import DataWriterQos, DataReaderQos, Reliability, History


@dataclass
class Keyed:
    _dds_type_name: ClassVar[str] = "InstSerKeyed"
    _extensibility: ClassVar[Extensibility] = Extensibility.FINAL
    _has_key: ClassVar[bool] = True

    sensor_id: int = 0    # @key u32
    value: float = 0.0    # f64

    def _serialize_cdr(self, xcdr2: bool = False) -> bytes:
        w = CdrWriter(extensibility=self._extensibility)
        w.write_u32(self.sensor_id)
        w.write_f64(self.value)
        return w.to_bytes()

    @classmethod
    def _deserialize_cdr(cls, data: bytes) -> "Keyed":
        r = CdrReader(data)
        return cls(sensor_id=r.read_u32(), value=r.read_f64())

    def _serialize_key(self) -> bytes:
        w = CdrKeyWriter()
        w.write_u32(self.sensor_id)
        return w.to_bytes()


DOMAIN = 96


@pytest.fixture
def reader_writer():
    dp = DomainParticipant(domain_id=DOMAIN)
    topic = dp.create_topic("InstSerTopic", Keyed)
    pub = dp.create_publisher()
    sub = dp.create_subscriber()
    wq = DataWriterQos(reliability=Reliability("RELIABLE"), history=History("KEEP_ALL"))
    rq = DataReaderQos(reliability=Reliability("RELIABLE"), history=History("KEEP_ALL"))
    writer = pub.create_datawriter(topic, qos=wq)
    reader = sub.create_datareader(topic, qos=rq)
    deadline = time.time() + 8
    while time.time() < deadline:
        if writer.matched_readers >= 1 and reader.matched_writers >= 1:
            break
        time.sleep(0.1)
    assert writer.matched_readers >= 1, "writer never matched reader"
    try:
        yield reader, writer
    finally:
        dp.close()


def _handles_by_sensor(reader):
    """Peek (read) the cache and map sensor_id -> instance_handle."""
    rc = reader.create_read_condition()
    try:
        by_sensor = {}
        for s in reader.read_w_condition(rc):
            if s.valid_data:
                by_sensor.setdefault(s.data.sensor_id, s.instance_handle)
        return by_sensor
    finally:
        rc.close()


def test_take_instance_serialized_scopes_to_one_instance(reader_writer):
    reader, writer = reader_writer
    writer.write(Keyed(sensor_id=1, value=10.0))
    writer.write(Keyed(sensor_id=1, value=11.0))
    writer.write(Keyed(sensor_id=2, value=20.0))
    time.sleep(1.0)

    handles = _handles_by_sensor(reader)
    ha, hb = handles.get(1), handles.get(2)
    assert ha and hb and ha != hb, f"expected two distinct instances, got {handles}"

    took_a = reader.take_instance_serialized(ha)
    assert sorted(s.data.value for s in took_a if s.valid_data) == [10.0, 11.0]

    # Instance B must still be takeable — take of A must not consume B's samples.
    took_b = reader.take_instance_serialized(hb)
    assert sorted(s.data.value for s in took_b if s.valid_data) == [20.0]


def test_read_instance_serialized_leaves_samples(reader_writer):
    reader, writer = reader_writer
    writer.write(Keyed(sensor_id=7, value=70.0))
    time.sleep(1.0)

    h7 = _handles_by_sensor(reader).get(7)
    assert h7
    # read (not take) leaves the sample; a second read still sees it.
    first = reader.read_instance_serialized(h7)
    assert [s.data.value for s in first if s.valid_data] == [70.0]
    second = reader.read_instance_serialized(h7)
    assert [s.data.value for s in second if s.valid_data] == [70.0]


def test_unknown_handle_returns_empty(reader_writer):
    reader, writer = reader_writer
    writer.write(Keyed(sensor_id=1, value=1.0))
    time.sleep(0.5)
    # Unknown handles yield no samples, not an error (mirrors the typed
    # read/take_instance path at the FFI boundary — a true NIL handle, with the
    # internal is_defined=false sentinel, is unreachable across the FFI, so an
    # all-zero handle is treated as merely unknown here).
    bogus = bytes([0xAB]) + b"\x00" * 15
    assert reader.take_instance_serialized(bogus) == []
    assert reader.take_instance_serialized(b"\x00" * 16) == []


def test_wrong_handle_length_raises(reader_writer):
    reader, _ = reader_writer
    with pytest.raises(ValueError):
        reader.take_instance_serialized(b"\x00" * 8)
