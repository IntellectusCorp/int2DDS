"""Tests for TOPIC_DATA QoS: Topic read-back and discovered endpoint data."""

import time
from dataclasses import dataclass
from typing import ClassVar

from int2dds.cdr import CdrReader, CdrWriter, Extensibility
from int2dds.core.participant import DomainParticipant
from int2dds.core.publisher import Publisher
from int2dds.core.qos import TopicData, TopicQos
from int2dds.core.subscriber import Subscriber


@dataclass
class TopicDataMsg:
    _dds_type_name: ClassVar[str] = "TopicDataMsg"
    _extensibility: ClassVar[Extensibility] = Extensibility.FINAL
    _has_key: ClassVar[bool] = False

    value: int = 0

    def _serialize_cdr(self, xcdr2: bool = False) -> bytes:
        w = CdrWriter(extensibility=self._extensibility)
        w.write_i32(self.value)
        return w.to_bytes()

    @classmethod
    def _deserialize_cdr(cls, data: bytes) -> "TopicDataMsg":
        return cls(value=CdrReader(data).read_i32())

    def _serialize_key(self) -> bytes:
        return b""


DOMAIN = 79


def _poll_discovered(snapshot_fn, topic_name: str, deadline_s: float = 10.0) -> dict | None:
    end = time.time() + deadline_s
    while True:
        mine = [d for d in snapshot_fn(timeout_ms=200) if d["topic_name"] == topic_name]
        if mine:
            return mine[0]
        if time.time() >= end:
            return None
        time.sleep(0.05)


def test_topic_data_read_back():
    dp = DomainParticipant(domain_id=DOMAIN)
    try:
        topic = dp.create_topic(
            "TopicDataReadBackTopic", TopicDataMsg, TopicQos(topic_data=TopicData(b"td-1")))
        assert topic.get_qos().topic_data == TopicData(b"td-1")

        topic.set_qos(TopicQos(topic_data=TopicData(b"td-2\x00\xff")))
        assert topic.get_qos().topic_data == TopicData(b"td-2\x00\xff")

        plain = dp.create_topic("TopicDataPlainTopic", TopicDataMsg)
        assert plain.get_qos().topic_data == TopicData(b"")
    finally:
        dp.close()


def test_discovered_endpoints_report_topic_data():
    pub_dp = DomainParticipant(domain_id=DOMAIN)
    sub_dp = DomainParticipant(domain_id=DOMAIN)
    try:
        pub_topic = pub_dp.create_topic(
            "TopicDataDiscoveryTopic", TopicDataMsg, TopicQos(topic_data=TopicData(b"pub-td")))
        sub_topic = sub_dp.create_topic(
            "TopicDataDiscoveryTopic", TopicDataMsg, TopicQos(topic_data=TopicData(b"sub-td")))
        pub = Publisher(pub_dp)
        sub = Subscriber(sub_dp)
        # Keep references: an unreferenced DataWriter/DataReader is deleted on GC.
        _writer = pub.create_datawriter(pub_topic)
        _reader = sub.create_datareader(sub_topic)

        publication = _poll_discovered(
            sub_dp.take_discovered_publications, "TopicDataDiscoveryTopic")
        subscription = _poll_discovered(
            pub_dp.take_discovered_subscriptions, "TopicDataDiscoveryTopic")
        assert publication is not None, "publication not discovered"
        assert subscription is not None, "subscription not discovered"
        assert publication["topic_data"] == b"pub-td"
        assert subscription["topic_data"] == b"sub-td"
    finally:
        pub_dp.close()
        sub_dp.close()
