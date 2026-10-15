"""E2E tests for ignore_participant, ignore_publication and ignore_subscription.

Each operation rejects a local handle, unmatches the remote entity, keeps it
unmatched after it re-announces itself through set_qos, and no data flows to or
from it.
"""

import time
from collections.abc import Callable
from dataclasses import dataclass
from typing import ClassVar

import pytest

from int2dds.cdr import CdrReader, CdrWriter, Extensibility
from int2dds.core.participant import DomainParticipant
from int2dds.core.publisher import Publisher
from int2dds.core.qos import DataReaderQos, DataWriterQos, ParticipantQos, UserData
from int2dds.core.subscriber import Subscriber
from int2dds.exceptions import DdsInvalidArgument


@dataclass
class Msg:
    _dds_type_name: ClassVar[str] = "IgnoreMsg"
    _extensibility: ClassVar[Extensibility] = Extensibility.FINAL
    _has_key: ClassVar[bool] = False

    value: int = 0

    def _serialize_cdr(self, xcdr2: bool = False) -> bytes:
        w = CdrWriter(extensibility=self._extensibility)
        w.write_i32(self.value)
        return w.to_bytes()

    @classmethod
    def _deserialize_cdr(cls, data: bytes) -> "Msg":
        return cls(value=CdrReader(data).read_i32())

    def _serialize_key(self) -> bytes:
        return b""


MATCH_TIMEOUT_S = 5.0
NO_DATA_WINDOW_S = 1.0


def _wait_until(condition: Callable[[], bool], failure_message: str) -> None:
    deadline = time.time() + MATCH_TIMEOUT_S
    while not condition():
        assert time.time() < deadline, failure_message
        time.sleep(0.01)


def test_ignore_participant_unmatches_and_drops_its_data():
    with DomainParticipant(domain_id=78) as local_participant, \
            DomainParticipant(domain_id=78) as remote_participant:
        local_topic = local_participant.create_topic("IgnoreParticipantPy", Msg)
        remote_topic = remote_participant.create_topic("IgnoreParticipantPy", Msg)
        reader = Subscriber(local_participant).create_datareader(local_topic)
        remote_writer = Publisher(remote_participant).create_datawriter(remote_topic)

        with pytest.raises(DdsInvalidArgument):
            local_participant.ignore_participant(local_participant.get_instance_handle())

        _wait_until(lambda: reader.get_subscription_matched_status()[1] == 1,
                    "reader did not match")

        local_participant.ignore_participant(remote_participant.get_instance_handle())
        _wait_until(lambda: reader.get_subscription_matched_status()[1] == 0,
                    "reader still matched after ignore")

        remote_participant.set_qos(ParticipantQos(user_data=UserData(data=b"\x01")))
        remote_writer.write(Msg(value=1))

        time.sleep(NO_DATA_WINDOW_S)
        assert reader.take() == []
        assert reader.get_subscription_matched_status()[1] == 0


def test_ignore_publication_unmatches_and_drops_its_data():
    with DomainParticipant(domain_id=79) as local_participant, \
            DomainParticipant(domain_id=79) as remote_participant:
        local_topic = local_participant.create_topic("IgnorePublicationPy", Msg)
        local_only_topic = local_participant.create_topic("IgnorePublicationLocalPy", Msg)
        remote_topic = remote_participant.create_topic("IgnorePublicationPy", Msg)
        reader = Subscriber(local_participant).create_datareader(local_topic)
        local_writer = Publisher(local_participant).create_datawriter(local_only_topic)
        remote_writer = Publisher(remote_participant).create_datawriter(remote_topic)

        with pytest.raises(DdsInvalidArgument):
            local_participant.ignore_publication(local_writer.get_instance_handle())

        _wait_until(lambda: reader.get_subscription_matched_status()[1] == 1,
                    "reader did not match")

        local_participant.ignore_publication(remote_writer.get_instance_handle())
        _wait_until(lambda: reader.get_subscription_matched_status()[1] == 0,
                    "reader still matched after ignore")

        remote_writer.set_qos(DataWriterQos(user_data=UserData(data=b"\x01")))
        remote_writer.write(Msg(value=1))

        time.sleep(NO_DATA_WINDOW_S)
        assert reader.take() == []
        assert reader.get_subscription_matched_status()[1] == 0


def test_ignore_subscription_unmatches_and_sends_it_no_data():
    with DomainParticipant(domain_id=80) as local_participant, \
            DomainParticipant(domain_id=80) as remote_participant:
        local_topic = local_participant.create_topic("IgnoreSubscriptionPy", Msg)
        local_only_topic = local_participant.create_topic("IgnoreSubscriptionLocalPy", Msg)
        remote_topic = remote_participant.create_topic("IgnoreSubscriptionPy", Msg)
        writer = Publisher(local_participant).create_datawriter(local_topic)
        local_reader = Subscriber(local_participant).create_datareader(local_only_topic)
        remote_reader = Subscriber(remote_participant).create_datareader(remote_topic)

        with pytest.raises(DdsInvalidArgument):
            local_participant.ignore_subscription(local_reader.get_instance_handle())

        _wait_until(lambda: writer.get_publication_matched_status()[1] == 1,
                    "writer did not match")

        local_participant.ignore_subscription(remote_reader.get_instance_handle())
        _wait_until(lambda: writer.get_publication_matched_status()[1] == 0,
                    "writer still matched after ignore")

        remote_reader.set_qos(DataReaderQos(user_data=UserData(data=b"\x01")))
        writer.write(Msg(value=1))

        time.sleep(NO_DATA_WINDOW_S)
        assert remote_reader.take() == []
        assert writer.get_publication_matched_status()[1] == 0
