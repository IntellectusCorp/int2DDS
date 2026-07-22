"""E2E tests for StatusCondition / get_status_changes across all DDS entities.

Verifies #333 A(5): get_statuscondition() and get_status_changes() are exposed on
DomainParticipant, Publisher, Subscriber, Topic (in addition to DataReader/DataWriter).
"""

import os
import sys
from dataclasses import dataclass
from typing import ClassVar

sys.path.insert(0, os.path.dirname(__file__))

from int2dds.cdr import CdrReader, CdrWriter, Extensibility
from int2dds.core.conditions import (
    WaitSet,
    STATUS_DATA_AVAILABLE,
    STATUS_INCONSISTENT_TOPIC,
    STATUS_PUBLICATION_MATCHED,
    STATUS_SUBSCRIPTION_MATCHED,
)
from int2dds.core.participant import DomainParticipant
from int2dds.core.publisher import Publisher
from int2dds.core.subscriber import Subscriber


@dataclass
class Msg:
    _dds_type_name: ClassVar[str] = "SCMsg"
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


DOMAIN = 74


def test_participant_statuscondition():
    p = DomainParticipant(domain_id=DOMAIN)
    sc = p.get_statuscondition()
    sc.set_enabled_statuses(STATUS_DATA_AVAILABLE)
    assert sc.enabled_statuses == STATUS_DATA_AVAILABLE
    assert isinstance(sc.trigger_value, bool)
    assert isinstance(p.get_status_changes(), int)
    ws = WaitSet()
    ws.attach(sc)
    ws.detach(sc)
    sc.close()


def test_publisher_statuscondition():
    p = DomainParticipant(domain_id=DOMAIN)
    pub = Publisher(p)
    sc = pub.get_statuscondition()
    sc.set_enabled_statuses(STATUS_PUBLICATION_MATCHED)
    assert sc.enabled_statuses == STATUS_PUBLICATION_MATCHED
    assert isinstance(pub.get_status_changes(), int)
    sc.close()


def test_subscriber_statuscondition():
    p = DomainParticipant(domain_id=DOMAIN)
    sub = Subscriber(p)
    sc = sub.get_statuscondition()
    sc.set_enabled_statuses(STATUS_SUBSCRIPTION_MATCHED)
    assert sc.enabled_statuses == STATUS_SUBSCRIPTION_MATCHED
    assert isinstance(sub.get_status_changes(), int)
    sc.close()


def test_topic_statuscondition():
    p = DomainParticipant(domain_id=DOMAIN)
    topic = p.create_topic("SCEntityTopic", Msg)
    sc = topic.get_statuscondition()
    sc.set_enabled_statuses(STATUS_INCONSISTENT_TOPIC)
    assert sc.enabled_statuses == STATUS_INCONSISTENT_TOPIC
    assert isinstance(topic.get_status_changes(), int)
    sc.close()


def test_reader_writer_status_changes():
    p = DomainParticipant(domain_id=DOMAIN)
    topic = p.create_topic("SCRWTopic", Msg)
    pub = Publisher(p)
    sub = Subscriber(p)
    writer = pub.create_datawriter(topic)
    reader = sub.create_datareader(topic)
    assert isinstance(writer.get_status_changes(), int)
    assert isinstance(reader.get_status_changes(), int)
    wsc = writer.get_statuscondition()
    rsc = reader.get_statuscondition()
    assert isinstance(wsc.trigger_value, bool)
    assert isinstance(rsc.trigger_value, bool)
    wsc.close()
    rsc.close()
