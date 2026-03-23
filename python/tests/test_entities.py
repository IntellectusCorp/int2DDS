"""
Tests for DDS entities (DomainParticipant, Publisher, Subscriber, etc.)

These tests require the int2dds_ffi library to be available.
Set INT2DDS_FFI_PATH environment variable if the library is not in the standard paths.
"""

from dataclasses import dataclass
from typing import ClassVar

import pytest

from int2dds.cdr import CdrReader, CdrWriter, Extensibility

# Skip all tests if FFI library is not available
try:
    from int2dds import (
        DataReader,
        DataWriter,
        DomainParticipant,
        Publisher,
        Sample,
        Subscriber,
        Topic,
        WaitSet,
    )

    FFI_AVAILABLE = True
except ImportError:
    FFI_AVAILABLE = False

pytestmark = pytest.mark.skipif(not FFI_AVAILABLE, reason="FFI library not available")


# Test data type
@dataclass
class TestMessage:
    """Simple test message type."""

    _dds_type_name: ClassVar[str] = "TestMessage"
    _extensibility: ClassVar[Extensibility] = Extensibility.FINAL
    _has_key: ClassVar[bool] = False

    value: int = 0
    text: str = ""

    def _serialize_cdr(self) -> bytes:
        w = CdrWriter(extensibility=self._extensibility)
        w.write_i32(self.value)
        w.write_string(self.text)
        return w.to_bytes()

    @classmethod
    def _deserialize_cdr(cls, data: bytes) -> "TestMessage":
        r = CdrReader(data)
        return cls(value=r.read_i32(), text=r.read_string())

    def _serialize_key(self) -> bytes:
        return b""


class TestDomainParticipant:
    """Tests for DomainParticipant."""

    def test_create_participant(self, domain_id: int):
        dp = DomainParticipant(domain_id=domain_id)
        assert dp.domain_id == domain_id
        dp.close()

    def test_context_manager(self, domain_id: int):
        with DomainParticipant(domain_id=domain_id) as dp:
            assert dp.domain_id == domain_id
        # Should be closed after exiting context

    def test_create_publisher(self, domain_id: int):
        with DomainParticipant(domain_id=domain_id) as dp:
            pub = dp.create_publisher()
            assert pub is not None
            pub.close()

    def test_create_subscriber(self, domain_id: int):
        with DomainParticipant(domain_id=domain_id) as dp:
            sub = dp.create_subscriber()
            assert sub is not None
            sub.close()

    def test_create_topic(self, domain_id: int, topic_name: str):
        with DomainParticipant(domain_id=domain_id) as dp:
            topic = dp.create_topic(topic_name, TestMessage)
            assert topic.name == topic_name
            assert topic.type_name == "TestMessage"
            topic.close()


class TestPublisher:
    """Tests for Publisher and DataWriter."""

    def test_create_datawriter(self, domain_id: int, topic_name: str):
        with DomainParticipant(domain_id=domain_id) as dp:
            topic = dp.create_topic(topic_name, TestMessage)
            pub = dp.create_publisher()
            writer = pub.create_datawriter(topic)
            assert writer is not None
            assert writer.topic == topic

    def test_write_sample(self, domain_id: int, topic_name: str):
        with DomainParticipant(domain_id=domain_id) as dp:
            topic = dp.create_topic(topic_name, TestMessage)
            pub = dp.create_publisher()
            writer = pub.create_datawriter(topic)

            sample = TestMessage(value=42, text="Hello")
            writer.write(sample)  # Should not raise


class TestSubscriber:
    """Tests for Subscriber and DataReader."""

    def test_create_datareader(self, domain_id: int, topic_name: str):
        with DomainParticipant(domain_id=domain_id) as dp:
            topic = dp.create_topic(topic_name, TestMessage)
            sub = dp.create_subscriber()
            reader = sub.create_datareader(topic)
            assert reader is not None
            assert reader.topic == topic

    def test_take_empty(self, domain_id: int, topic_name: str):
        with DomainParticipant(domain_id=domain_id) as dp:
            topic = dp.create_topic(topic_name, TestMessage)
            sub = dp.create_subscriber()
            reader = sub.create_datareader(topic)

            samples = reader.take()
            assert samples == []  # No data yet


class TestWaitSet:
    """Tests for WaitSet."""

    def test_create_waitset(self):
        waitset = WaitSet()
        assert waitset is not None
        waitset.close()

    def test_attach_detach_datareader(self, domain_id: int, topic_name: str):
        with DomainParticipant(domain_id=domain_id) as dp:
            topic = dp.create_topic(topic_name, TestMessage)
            sub = dp.create_subscriber()
            reader = sub.create_datareader(topic)

            waitset = WaitSet()
            waitset.attach(reader)
            waitset.detach(reader)
            waitset.close()

    def test_wait_timeout(self, domain_id: int, topic_name: str):
        from int2dds import DdsTimeout

        with DomainParticipant(domain_id=domain_id) as dp:
            topic = dp.create_topic(topic_name, TestMessage)
            sub = dp.create_subscriber()
            reader = sub.create_datareader(topic)

            waitset = WaitSet()
            waitset.attach(reader)

            # Should timeout since no data
            with pytest.raises(DdsTimeout):
                waitset.wait(timeout=0.1)

            waitset.close()


class TestMatchedStatus:
    """Tests for matched status APIs."""

    def test_writer_matched_readers(self, domain_id: int, topic_name: str):
        with DomainParticipant(domain_id=domain_id) as dp:
            topic = dp.create_topic(topic_name, TestMessage)
            pub = dp.create_publisher()
            writer = pub.create_datawriter(topic)

            # Initially no matched readers
            assert writer.matched_readers == 0

    def test_reader_matched_writers(self, domain_id: int, topic_name: str):
        with DomainParticipant(domain_id=domain_id) as dp:
            topic = dp.create_topic(topic_name, TestMessage)
            sub = dp.create_subscriber()
            reader = sub.create_datareader(topic)

            # Initially no matched writers
            assert reader.matched_writers == 0
