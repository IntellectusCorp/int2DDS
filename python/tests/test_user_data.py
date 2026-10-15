"""Tests for USER_DATA QoS read-back on DomainParticipant, DataWriter and DataReader."""

from dataclasses import dataclass
from typing import ClassVar

from int2dds.cdr import CdrReader, CdrWriter, Extensibility
from int2dds.core.participant import DomainParticipant
from int2dds.core.publisher import Publisher
from int2dds.core.qos import DataReaderQos, DataWriterQos, ParticipantQos, UserData
from int2dds.core.subscriber import Subscriber


@dataclass
class UserDataMsg:
    _dds_type_name: ClassVar[str] = "UserDataMsg"
    _extensibility: ClassVar[Extensibility] = Extensibility.FINAL
    _has_key: ClassVar[bool] = False

    value: int = 0

    def _serialize_cdr(self, xcdr2: bool = False) -> bytes:
        w = CdrWriter(extensibility=self._extensibility)
        w.write_i32(self.value)
        return w.to_bytes()

    @classmethod
    def _deserialize_cdr(cls, data: bytes) -> "UserDataMsg":
        return cls(value=CdrReader(data).read_i32())

    def _serialize_key(self) -> bytes:
        return b""


DOMAIN = 78


def test_participant_user_data_read_back():
    dp = DomainParticipant(domain_id=DOMAIN, qos=ParticipantQos(user_data=UserData(b"part-1")))
    try:
        assert dp.get_qos().user_data == UserData(b"part-1")

        dp.set_qos(ParticipantQos(user_data=UserData(b"part-2\x00\xff")))
        assert dp.get_qos().user_data == UserData(b"part-2\x00\xff")

        dp.set_qos(ParticipantQos(user_data=UserData(b"")))
        assert dp.get_qos().user_data == UserData(b"")
    finally:
        dp.close()


def test_participant_user_data_unset_reads_back_empty():
    dp = DomainParticipant(domain_id=DOMAIN)
    try:
        assert dp.get_qos().user_data == UserData(b"")
    finally:
        dp.close()


def test_datawriter_datareader_user_data_read_back():
    dp = DomainParticipant(domain_id=DOMAIN)
    try:
        topic = dp.create_topic("UserDataReadBackTopic", UserDataMsg)
        pub = Publisher(dp)
        sub = Subscriber(dp)

        writer = pub.create_datawriter(topic, DataWriterQos(user_data=UserData(b"writer")))
        reader = sub.create_datareader(topic, DataReaderQos(user_data=UserData(b"reader")))
        assert writer.get_qos().user_data == UserData(b"writer")
        assert reader.get_qos().user_data == UserData(b"reader")

        plain_writer = pub.create_datawriter(topic)
        plain_reader = sub.create_datareader(topic)
        assert plain_writer.get_qos().user_data == UserData(b"")
        assert plain_reader.get_qos().user_data == UserData(b"")
    finally:
        dp.close()
