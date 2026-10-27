"""Tests for PRESENTATION QoS, coherent sets and Subscriber.get_datareaders.

The end-to-end test runs one publishing and one subscribing participant on a
dedicated domain, both with GROUP access scope and coherent + ordered access.
"""

import time
from dataclasses import dataclass
from typing import ClassVar

import pytest

from int2dds.cdr import CdrReader, CdrWriter, Extensibility
from int2dds.core.participant import DomainParticipant
from int2dds.core.publisher import Publisher
from int2dds.core.qos import (
    DataReaderQos,
    DataWriterQos,
    History,
    Presentation,
    PublisherQos,
    Reliability,
    SubscriberQos,
)
from int2dds.core.subscriber import Subscriber
from int2dds.exceptions import DdsPreconditionNotMet


@dataclass
class PresMsg:
    _dds_type_name: ClassVar[str] = "PresMsg"
    _extensibility: ClassVar[Extensibility] = Extensibility.FINAL
    _has_key: ClassVar[bool] = False

    value: int = 0

    def _serialize_cdr(self, xcdr2: bool = False) -> bytes:
        w = CdrWriter(extensibility=self._extensibility)
        w.write_i32(self.value)
        return w.to_bytes()

    @classmethod
    def _deserialize_cdr(cls, data: bytes) -> "PresMsg":
        return cls(value=CdrReader(data).read_i32())

    def _serialize_key(self) -> bytes:
        return b""


DOMAIN = 78
GROUP_PRESENTATION = Presentation(access_scope="GROUP", coherent_access=True, ordered_access=True)


def _wait_until(predicate, deadline_s: float = 10.0) -> bool:
    end = time.time() + deadline_s
    while not predicate():
        if time.time() >= end:
            return False
        time.sleep(0.05)
    return True


def test_publisher_subscriber_presentation_read_back():
    dp = DomainParticipant(domain_id=DOMAIN)
    try:
        pub = Publisher(dp, PublisherQos(presentation=GROUP_PRESENTATION))
        sub = Subscriber(dp, SubscriberQos(presentation=Presentation(
            access_scope="TOPIC", coherent_access=True, ordered_access=False)))

        assert pub.get_qos().presentation == GROUP_PRESENTATION
        assert sub.get_qos().presentation == Presentation(
            access_scope="TOPIC", coherent_access=True, ordered_access=False)
        assert Publisher(dp).get_qos().presentation == Presentation()
    finally:
        dp.close()


def test_group_coherent_ordered_get_datareaders():
    pub_dp = DomainParticipant(domain_id=DOMAIN)
    sub_dp = DomainParticipant(domain_id=DOMAIN)
    try:
        pub = Publisher(pub_dp, PublisherQos(presentation=GROUP_PRESENTATION))
        sub = Subscriber(sub_dp, SubscriberQos(presentation=GROUP_PRESENTATION))
        writer_qos = DataWriterQos(
            reliability=Reliability("RELIABLE"), history=History("KEEP_ALL"))
        reader_qos = DataReaderQos(
            reliability=Reliability("RELIABLE"), history=History("KEEP_ALL"))

        writer_a = pub.create_datawriter(
            pub_dp.create_topic("PresGroupTopicA", PresMsg), writer_qos)
        writer_b = pub.create_datawriter(
            pub_dp.create_topic("PresGroupTopicB", PresMsg), writer_qos)
        reader_a = sub.create_datareader(
            sub_dp.create_topic("PresGroupTopicA", PresMsg), reader_qos)
        reader_b = sub.create_datareader(
            sub_dp.create_topic("PresGroupTopicB", PresMsg), reader_qos)

        assert _wait_until(
            lambda: writer_a.matched_readers >= 1 and writer_b.matched_readers >= 1), \
            "writers did not match within the deadline"

        with pytest.raises(DdsPreconditionNotMet):
            pub.end_coherent_changes()
        with pytest.raises(DdsPreconditionNotMet):
            sub.end_access()
        with pytest.raises(DdsPreconditionNotMet):
            sub.get_datareaders()

        pub.begin_coherent_changes()
        writer_a.write(PresMsg(value=1))
        writer_b.write(PresMsg(value=2))
        writer_a.write(PresMsg(value=3))
        pub.end_coherent_changes()

        readers = []

        def _three_entries() -> bool:
            nonlocal readers
            sub.begin_access()
            try:
                readers = sub.get_datareaders()
            finally:
                sub.end_access()
            return len(readers) >= 3

        assert _wait_until(_three_entries), f"got {len(readers)} entries"
        assert len(readers) == 3
        assert readers[0] is reader_a
        assert readers[1] is reader_b
        assert readers[2] is reader_a

        # GROUP access scope rejects a take outside an access block.
        with pytest.raises(DdsPreconditionNotMet):
            reader_a.take_one()

        # Taking one sample per listed reader, in list order, yields the write order.
        values = []
        sub.begin_access()
        try:
            for reader in sub.get_datareaders():
                sample = reader.take_one()
                assert sample is not None
                values.append(sample.data.value)
        finally:
            sub.end_access()

        assert values == [1, 2, 3]

        publications = []

        def _publication_a_discovered() -> bool:
            publications[:] = [
                d for d in sub_dp.take_discovered_publications(timeout_ms=200)
                if d["topic_name"] == "PresGroupTopicA"]
            return bool(publications)

        assert _wait_until(_publication_a_discovered), "publication A not discovered"
        d = publications[0]
        assert d["presentation_access_scope"] == 2
        assert d["presentation_coherent_access"] is True
        assert d["presentation_ordered_access"] is True
    finally:
        pub_dp.close()
        sub_dp.close()
