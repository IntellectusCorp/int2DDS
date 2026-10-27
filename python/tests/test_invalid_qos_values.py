"""Invalid QoS values are rejected when an entity is created; valid ones still apply."""

from dataclasses import dataclass
from typing import ClassVar

import pytest

from int2dds.cdr import CdrReader, CdrWriter, Extensibility
from int2dds.core.participant import DomainParticipant
from int2dds.core.qos import DataReaderQos, DataWriterQos, History, Reliability
from int2dds.exceptions import DdsInvalidArgument


@dataclass
class InvalidQosMsg:
    _dds_type_name: ClassVar[str] = "InvalidQosMsg"
    _extensibility: ClassVar[Extensibility] = Extensibility.FINAL
    _has_key: ClassVar[bool] = False

    value: int = 0

    def _serialize_cdr(self, xcdr2: bool = False) -> bytes:
        w = CdrWriter(extensibility=self._extensibility)
        w.write_i32(self.value)
        return w.to_bytes()

    @classmethod
    def _deserialize_cdr(cls, data: bytes) -> "InvalidQosMsg":
        return cls(value=CdrReader(data).read_i32())

    def _serialize_key(self) -> bytes:
        return b""


def test_invalid_qos_values_are_rejected_and_valid_ones_apply():
    with DomainParticipant(domain_id=84) as dp:
        topic = dp.create_topic("InvalidQosTopic", InvalidQosMsg)
        pub = dp.create_publisher()
        sub = dp.create_subscriber()

        with pytest.raises(ValueError):
            pub.create_datawriter(topic, qos=DataWriterQos(reliability=Reliability("reliable")))
        with pytest.raises(DdsInvalidArgument):
            pub.create_datawriter(
                topic, qos=DataWriterQos(reliability=Reliability("RELIABLE", max_blocking_time=-1.0)))

        reliable = Reliability("RELIABLE", max_blocking_time=0.2)
        pub.create_datawriter(topic, qos=DataWriterQos(reliability=reliable, history=History("KEEP_ALL")))
        reader = sub.create_datareader(
            topic, qos=DataReaderQos(reliability=reliable, history=History("KEEP_ALL")))
        assert reader.get_qos().reliability.max_blocking_time == pytest.approx(0.2)
