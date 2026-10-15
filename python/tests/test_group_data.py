"""Tests for GROUP_DATA QoS: Publisher/Subscriber read-back and discovered endpoint data."""

import time
from dataclasses import dataclass
from typing import ClassVar

from int2dds.cdr import CdrReader, CdrWriter, Extensibility
from int2dds.core.participant import DomainParticipant
from int2dds.core.publisher import Publisher
from int2dds.core.qos import GroupData, PublisherQos, SubscriberQos
from int2dds.core.subscriber import Subscriber


@dataclass
class GroupDataMsg:
    _dds_type_name: ClassVar[str] = "GroupDataMsg"
    _extensibility: ClassVar[Extensibility] = Extensibility.FINAL
    _has_key: ClassVar[bool] = False

    value: int = 0

    def _serialize_cdr(self, xcdr2: bool = False) -> bytes:
        w = CdrWriter(extensibility=self._extensibility)
        w.write_i32(self.value)
        return w.to_bytes()

    @classmethod
    def _deserialize_cdr(cls, data: bytes) -> "GroupDataMsg":
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


def test_publisher_subscriber_group_data_read_back():
    dp = DomainParticipant(domain_id=DOMAIN)
    try:
        pub = Publisher(dp, PublisherQos(group_data=GroupData(b"pub-gd-1")))
        sub = Subscriber(dp, SubscriberQos(group_data=GroupData(b"sub-gd-1")))
        assert pub.get_qos().group_data == GroupData(b"pub-gd-1")
        assert sub.get_qos().group_data == GroupData(b"sub-gd-1")

        pub.set_qos(PublisherQos(group_data=GroupData(b"pub-gd-2\x00\xff")))
        sub.set_qos(SubscriberQos(group_data=GroupData(b"sub-gd-2")))
        assert pub.get_qos().group_data == GroupData(b"pub-gd-2\x00\xff")
        assert sub.get_qos().group_data == GroupData(b"sub-gd-2")

        assert Publisher(dp).get_qos().group_data == GroupData(b"")
        assert Subscriber(dp).get_qos().group_data == GroupData(b"")
    finally:
        dp.close()


def test_discovered_endpoints_report_group_data():
    pub_dp = DomainParticipant(domain_id=DOMAIN)
    sub_dp = DomainParticipant(domain_id=DOMAIN)
    try:
        pub = Publisher(pub_dp, PublisherQos(group_data=GroupData(b"pub-gd")))
        sub = Subscriber(sub_dp, SubscriberQos(group_data=GroupData(b"sub-gd")))
        # Keep references: an unreferenced DataWriter/DataReader is deleted on GC.
        _writer = pub.create_datawriter(
            pub_dp.create_topic("GroupDataDiscoveryTopic", GroupDataMsg))
        _reader = sub.create_datareader(
            sub_dp.create_topic("GroupDataDiscoveryTopic", GroupDataMsg))

        publication = _poll_discovered(
            sub_dp.take_discovered_publications, "GroupDataDiscoveryTopic")
        subscription = _poll_discovered(
            pub_dp.take_discovered_subscriptions, "GroupDataDiscoveryTopic")
        assert publication is not None, "publication not discovered"
        assert subscription is not None, "subscription not discovered"
        assert publication["group_data"] == b"pub-gd"
        assert subscription["group_data"] == b"sub-gd"
    finally:
        pub_dp.close()
        sub_dp.close()
