"""
Tests for DDS entities (DomainParticipant, Publisher, Subscriber, etc.)

These tests require the int2dds_ffi library to be available.
Set INT2DDS_FFI_PATH environment variable if the library is not in the standard paths.
"""

from dataclasses import dataclass, field
from enum import IntEnum
from typing import ClassVar

import os
import sys
import pytest

from int2dds.cdr import CdrReader, CdrWriter, Extensibility
from int2dds.cdr.writer import CdrKeyWriter

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

    def _serialize_cdr(self, xcdr2: bool = True) -> bytes:
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


@dataclass
class AllPrimitives:
    """Test struct covering all CDR primitive types."""

    _dds_type_name: ClassVar[str] = "AllPrimitives"
    _extensibility: ClassVar[Extensibility] = Extensibility.FINAL
    _has_key: ClassVar[bool] = False

    flag: bool = False
    byte_val: int = 0       # u8
    ibyte_val: int = 0      # i8
    short_val: int = 0      # i16
    ushort_val: int = 0     # u16
    long_val: int = 0       # i32
    ulong_val: int = 0      # u32
    llong_val: int = 0      # i64
    ullong_val: int = 0     # u64
    float_val: float = 0.0  # f32
    double_val: float = 0.0 # f64
    char_val: str = ""      # char
    text: str = ""          # string

    def _serialize_cdr(self, xcdr2: bool = True) -> bytes:
        w = CdrWriter(extensibility=self._extensibility)
        w.write_bool(self.flag)
        w.write_u8(self.byte_val)
        w.write_i8(self.ibyte_val)
        w.write_i16(self.short_val)
        w.write_u16(self.ushort_val)
        w.write_i32(self.long_val)
        w.write_u32(self.ulong_val)
        w.write_i64(self.llong_val)
        w.write_u64(self.ullong_val)
        w.write_f32(self.float_val)
        w.write_f64(self.double_val)
        w.write_char(self.char_val)
        w.write_string(self.text)
        return w.to_bytes()

    @classmethod
    def _deserialize_cdr(cls, data: bytes) -> "AllPrimitives":
        r = CdrReader(data)
        return cls(
            flag=r.read_bool(),
            byte_val=r.read_u8(),
            ibyte_val=r.read_i8(),
            short_val=r.read_i16(),
            ushort_val=r.read_u16(),
            long_val=r.read_i32(),
            ulong_val=r.read_u32(),
            llong_val=r.read_i64(),
            ullong_val=r.read_u64(),
            float_val=r.read_f32(),
            double_val=r.read_f64(),
            char_val=r.read_char(),
            text=r.read_string(),
        )

    def _serialize_key(self) -> bytes:
        return b""


ARRAY_SIZE = 5


@dataclass
class ArrayType:
    """Test struct with fixed-size array."""

    _dds_type_name: ClassVar[str] = "ArrayType"
    _extensibility: ClassVar[Extensibility] = Extensibility.FINAL
    _has_key: ClassVar[bool] = False

    values: list[int] = field(default_factory=lambda: [0] * ARRAY_SIZE)  # u32[5]

    def _serialize_cdr(self, xcdr2: bool = True) -> bytes:
        w = CdrWriter(extensibility=self._extensibility)
        assert len(self.values) == ARRAY_SIZE, "Array size mismatch"
        for v in self.values:
            w.write_u32(v)
        return w.to_bytes()

    @classmethod
    def _deserialize_cdr(cls, data: bytes) -> "ArrayType":
        r = CdrReader(data)
        values = [r.read_u32() for _ in range(ARRAY_SIZE)]
        return cls(values=values)

    def _serialize_key(self) -> bytes:
        return b""


@dataclass
class SequenceType:
    """Test struct with variable-length sequence."""

    _dds_type_name: ClassVar[str] = "SequenceType"
    _extensibility: ClassVar[Extensibility] = Extensibility.FINAL
    _has_key: ClassVar[bool] = False

    items: list[int] = field(default_factory=list)  # unbounded sequence of u32

    def _serialize_cdr(self, xcdr2: bool = True) -> bytes:
        w = CdrWriter(extensibility=self._extensibility)
        w.write_seq_header(len(self.items))
        for v in self.items:
            w.write_u32(v)
        return w.to_bytes()

    @classmethod
    def _deserialize_cdr(cls, data: bytes) -> "SequenceType":
        r = CdrReader(data)
        count = r.read_seq_header()
        items = [r.read_u32() for _ in range(count)]
        return cls(items=items)

    def _serialize_key(self) -> bytes:
        return b""


class Color(IntEnum):
    """IDL enum: Color"""

    RED = 0
    GREEN = 1
    BLUE = 2
    YELLOW = 3
    PURPLE = 4


class StatusKind(IntEnum):
    """IDL enum: StatusKind"""

    UNKNOWN = 0
    ACTIVE = 1
    INACTIVE = 2
    PENDING = 3


@dataclass
class EnumMessage:
    """IDL struct: EnumType"""

    _dds_type_name: ClassVar[str] = "EnumType"
    _extensibility: ClassVar[Extensibility] = Extensibility.FINAL
    _has_key: ClassVar[bool] = False

    id: int = 0
    color: Color = Color.RED
    status: StatusKind = StatusKind.UNKNOWN

    def _serialize_cdr(self, xcdr2: bool = True) -> bytes:
        w = CdrWriter(extensibility=self._extensibility)
        w.write_i32(self.id)
        w.write_enum(int(self.color))
        w.write_enum(int(self.status))
        return w.to_bytes()

    @classmethod
    def _deserialize_cdr(cls, data: bytes) -> "EnumMessage":
        r = CdrReader(data)
        return cls(
            id=r.read_i32(),
            color=Color(r.read_enum()),
            status=StatusKind(r.read_enum()),
        )

    def _serialize_key(self) -> bytes:
        return b""


@dataclass
class InnerStruct:
    """IDL struct: InnerStruct"""

    _dds_type_name: ClassVar[str] = "InnerStruct"
    _extensibility: ClassVar[Extensibility] = Extensibility.FINAL
    _has_key: ClassVar[bool] = False

    x: int = 0       # i32
    y: int = 0       # i32
    name: str = ""   # string

    def _serialize_cdr(self, xcdr2: bool = True) -> bytes:
        w = CdrWriter(extensibility=self._extensibility)
        w.write_i32(self.x)
        w.write_i32(self.y)
        w.write_string(self.name)
        return w.to_bytes()

    @classmethod
    def _deserialize_cdr(cls, data: bytes) -> "InnerStruct":
        r = CdrReader(data)
        return cls(x=r.read_i32(), y=r.read_i32(), name=r.read_string())

    @classmethod
    def _deserialize_fields(cls, r: CdrReader) -> "InnerStruct":
        """Deserialize fields from an existing reader (no encap header)."""
        return cls(x=r.read_i32(), y=r.read_i32(), name=r.read_string())

    def _serialize_key(self) -> bytes:
        return b""


@dataclass
class NestedType:
    """IDL struct: Nested_Structs"""

    _dds_type_name: ClassVar[str] = "NestedType"
    _extensibility: ClassVar[Extensibility] = Extensibility.FINAL
    _has_key: ClassVar[bool] = False

    id: int = 0                                              # i32
    inner: InnerStruct = field(default_factory=InnerStruct)  # nested struct

    def _serialize_cdr(self, xcdr2: bool = True) -> bytes:
        w = CdrWriter(extensibility=self._extensibility)
        w.write_i32(self.id)
        # Nested struct: write fields without encap header
        nested_bytes = self.inner._serialize_cdr()
        w.write_bytes(nested_bytes[4:])  # skip encap header
        return w.to_bytes()

    @classmethod
    def _deserialize_cdr(cls, data: bytes) -> "NestedType":
        r = CdrReader(data)
        id = r.read_i32()
        inner = InnerStruct._deserialize_fields(r)
        return cls(id=id, inner=inner)

    def _serialize_key(self) -> bytes:
        return b""


@dataclass
class KeyedType:
    """Test struct with @key field for instance management."""

    _dds_type_name: ClassVar[str] = "KeyedType"
    _extensibility: ClassVar[Extensibility] = Extensibility.FINAL
    _has_key: ClassVar[bool] = True

    sensor_id: int = 0    # @key u32
    value: float = 0.0    # f64

    def _serialize_cdr(self, xcdr2: bool = True) -> bytes:
        w = CdrWriter(extensibility=self._extensibility)
        w.write_u32(self.sensor_id)
        w.write_f64(self.value)
        return w.to_bytes()

    @classmethod
    def _deserialize_cdr(cls, data: bytes) -> "KeyedType":
        r = CdrReader(data)
        return cls(sensor_id=r.read_u32(), value=r.read_f64())

    def _serialize_key(self) -> bytes:
        w = CdrKeyWriter()
        w.write_u32(self.sensor_id)
        return w.to_bytes()


@dataclass
class AppendableMsg:
    """Test struct with APPENDABLE extensibility (DHEADER wrapping)."""

    _dds_type_name: ClassVar[str] = "AppendableMsg"
    _extensibility: ClassVar[Extensibility] = Extensibility.APPENDABLE
    _has_key: ClassVar[bool] = False

    id: int = 0      # i32
    value: float = 0.0  # f64

    def _serialize_cdr(self, xcdr2: bool = True) -> bytes:
        w = CdrWriter(extensibility=self._extensibility)
        with w.dheader():
            w.write_i32(self.id)
            w.write_f64(self.value)
        return w.to_bytes()

    @classmethod
    def _deserialize_cdr(cls, data: bytes) -> "AppendableMsg":
        r = CdrReader(data)
        _dsize, _dstart = r.read_dheader()
        id = r.read_i32()
        value = r.read_f64()
        r.read_dheader_end(_dsize, _dstart)
        return cls(id=id, value=value)

    def _serialize_key(self) -> bytes:
        return b""


@dataclass
class MutableMsg:
    """Test struct with MUTABLE extensibility (DHEADER + EMHEADER per field)."""

    _dds_type_name: ClassVar[str] = "MutableMsg"
    _extensibility: ClassVar[Extensibility] = Extensibility.MUTABLE
    _has_key: ClassVar[bool] = False

    id: int = 0         # member_id=0, i32
    value: float = 0.0  # member_id=1, f64

    def _serialize_cdr(self, xcdr2: bool = True) -> bytes:
        w = CdrWriter(extensibility=self._extensibility)
        with w.dheader():
            with w.emheader(member_id=0):
                w.write_i32(self.id)
            with w.emheader(member_id=1):
                w.write_f64(self.value)
            w.write_sentinel()
        return w.to_bytes()

    @classmethod
    def _deserialize_cdr(cls, data: bytes) -> "MutableMsg":
        r = CdrReader(data)
        _dsize, _dstart = r.read_dheader()
        id = 0
        value = 0.0
        end_pos = _dstart + _dsize
        while r.position < end_pos:
            if r.is_sentinel():
                r.skip_sentinel()
                break
            _mid, _mlen, _mu = r.read_emheader()
            if _mid == 0:
                id = r.read_i32()
            elif _mid == 1:
                value = r.read_f64()
            else:
                r.skip(_mlen)
        r.read_dheader_end(_dsize, _dstart)
        return cls(id=id, value=value)

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

    def test_create_keyed_topic(self, domain_id: int):
        with DomainParticipant(domain_id=domain_id) as dp:
            topic = dp.create_topic("KeyedTopic", KeyedType)
            assert topic.name == "KeyedTopic"
            assert topic.type_name == "KeyedType"
            topic.close()

    def test_double_close_participant(self, domain_id: int):
        dp = DomainParticipant(domain_id=domain_id)
        dp.close()
        dp.close()  # should not raise

    def test_double_close_topic(self, domain_id: int):
        with DomainParticipant(domain_id=domain_id) as dp:
            topic = dp.create_topic("DoubleCloseTopic", TestMessage)
            topic.close()
            topic.close()  # should not raise


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
            writer.write(sample)


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

    def test_writer_statuscondition_publication_matched(self, domain_id: int, topic_name: str):
        """Wait on the writer's own StatusCondition for PUBLICATION_MATCHED."""
        from int2dds import DdsTimeout
        from int2dds.core.conditions import STATUS_PUBLICATION_MATCHED

        with DomainParticipant(domain_id=domain_id) as dp:
            topic = dp.create_topic(topic_name, TestMessage)
            pub = dp.create_publisher()
            sub = dp.create_subscriber()
            writer = pub.create_datawriter(topic)
            reader = sub.create_datareader(topic)

            # Wait for discovery via the writer-side StatusCondition
            status_cond = writer.get_statuscondition()
            status_cond.set_enabled_statuses(STATUS_PUBLICATION_MATCHED)
            waitset = WaitSet()
            waitset.attach(status_cond)

            deadline = 5.0
            while writer.matched_readers == 0 and deadline > 0:
                try:
                    waitset.wait(timeout=1.0)
                except DdsTimeout:
                    pass
                deadline -= 1.0
            assert writer.matched_readers > 0, "Writer should match the reader"


class TestQoS:
    """QoS delivery verification tests."""

    def test_reliability_incompatible(self, domain_id: int):
        """BEST_EFFORT writer + RELIABLE reader should not match."""
        from int2dds.core.qos import DataWriterQos, DataReaderQos, Reliability

        with DomainParticipant(domain_id=domain_id) as dp:
            topic = dp.create_topic("QoS_Reliability_Topic", TestMessage)
            pub = dp.create_publisher()
            sub = dp.create_subscriber()

            writer_qos = DataWriterQos(reliability=Reliability("BEST_EFFORT"))
            reader_qos = DataReaderQos(reliability=Reliability("RELIABLE"))

            writer = pub.create_datawriter(topic, qos=writer_qos)
            reader = sub.create_datareader(topic, qos=reader_qos)

            import time
            time.sleep(1.0)

            # Should not match due to QoS incompatibility
            assert writer.matched_readers == 0
            assert reader.matched_writers == 0

    def test_reliability_compatible(self, domain_id: int):
        """RELIABLE writer + RELIABLE reader should match and communicate."""
        from int2dds import DdsTimeout
        from int2dds.core.qos import DataWriterQos, DataReaderQos, Reliability
        from int2dds.core.conditions import STATUS_SUBSCRIPTION_MATCHED, STATUS_DATA_AVAILABLE

        with DomainParticipant(domain_id=domain_id) as dp:
            topic = dp.create_topic("QoS_ReliableOK_Topic", TestMessage)
            pub = dp.create_publisher()
            sub = dp.create_subscriber()

            writer_qos = DataWriterQos(reliability=Reliability("RELIABLE"))
            reader_qos = DataReaderQos(reliability=Reliability("RELIABLE"))

            writer = pub.create_datawriter(topic, qos=writer_qos)
            reader = sub.create_datareader(topic, qos=reader_qos)

            # Wait for discovery
            status_cond = reader.get_statuscondition()
            status_cond.set_enabled_statuses(STATUS_SUBSCRIPTION_MATCHED)
            waitset = WaitSet()
            waitset.attach(status_cond)

            deadline = 5.0
            while writer.matched_readers == 0 and deadline > 0:
                try:
                    waitset.wait(timeout=1.0)
                except DdsTimeout:
                    pass
                deadline -= 1.0
            assert writer.matched_readers > 0, "RELIABLE/RELIABLE should match"

            # Write and read
            status_cond.set_enabled_statuses(STATUS_DATA_AVAILABLE)
            writer.write(TestMessage(value=100, text="reliable"))
            try:
                waitset.wait(timeout=5.0)
            except DdsTimeout:
                pass
            samples = reader.take()
            assert len(samples) > 0
            assert samples[0].data.value == 100

    def test_durability_transient_local(self, domain_id: int):
        """TRANSIENT_LOCAL: late-joining reader should receive previously written data."""
        from int2dds import DdsTimeout
        from int2dds.core.qos import (
            DataWriterQos, DataReaderQos, Reliability, Durability,
        )
        from int2dds.core.conditions import STATUS_SUBSCRIPTION_MATCHED, STATUS_DATA_AVAILABLE

        with DomainParticipant(domain_id=domain_id) as dp:
            topic = dp.create_topic("QoS_Durability_Topic", TestMessage)
            pub = dp.create_publisher()

            writer_qos = DataWriterQos(
                reliability=Reliability("RELIABLE"),
                durability=Durability("TRANSIENT_LOCAL"),
            )
            writer = pub.create_datawriter(topic, qos=writer_qos)

            # Write BEFORE reader exists
            writer.write(TestMessage(value=999, text="late join"))

            # Now create reader with TRANSIENT_LOCAL
            sub = dp.create_subscriber()
            reader_qos = DataReaderQos(
                reliability=Reliability("RELIABLE"),
                durability=Durability("TRANSIENT_LOCAL"),
            )
            reader = sub.create_datareader(topic, qos=reader_qos)

            # Wait for discovery first
            status_cond = reader.get_statuscondition()
            status_cond.set_enabled_statuses(STATUS_SUBSCRIPTION_MATCHED)
            waitset = WaitSet()
            waitset.attach(status_cond)

            deadline = 5.0
            while reader.matched_writers == 0 and deadline > 0:
                try:
                    waitset.wait(timeout=1.0)
                except DdsTimeout:
                    pass
                deadline -= 1.0
            assert reader.matched_writers > 0, "Discovery failed"

            # After discovery, wait for TRANSIENT_LOCAL data delivery
            status_cond.set_enabled_statuses(STATUS_DATA_AVAILABLE)
            deadline = 5.0
            samples = []
            while deadline > 0 and len(samples) == 0:
                try:
                    waitset.wait(timeout=1.0)
                except DdsTimeout:
                    pass
                samples = reader.take()
                deadline -= 1.0

            assert len(samples) > 0, "Late-joining reader should receive TRANSIENT_LOCAL data"
            assert samples[0].data.value == 999

    def test_durability_volatile(self, domain_id: int):
        """VOLATILE: late-joining reader should NOT receive previously written data."""
        from int2dds import DdsTimeout
        from int2dds.core.qos import DataWriterQos, DataReaderQos, Reliability, Durability
        from int2dds.core.conditions import STATUS_SUBSCRIPTION_MATCHED

        with DomainParticipant(domain_id=domain_id) as dp:
            topic = dp.create_topic("QoS_Volatile_Topic", TestMessage)
            pub = dp.create_publisher()

            writer_qos = DataWriterQos(
                reliability=Reliability("RELIABLE"),
                durability=Durability("VOLATILE"),
            )
            writer = pub.create_datawriter(topic, qos=writer_qos)

            # Write BEFORE reader exists
            writer.write(TestMessage(value=888, text="volatile"))

            # Now create reader with VOLATILE
            sub = dp.create_subscriber()
            reader_qos = DataReaderQos(
                reliability=Reliability("RELIABLE"),
                durability=Durability("VOLATILE"),
            )
            reader = sub.create_datareader(topic, qos=reader_qos)

            # Wait for discovery
            status_cond = reader.get_statuscondition()
            status_cond.set_enabled_statuses(STATUS_SUBSCRIPTION_MATCHED)
            waitset = WaitSet()
            waitset.attach(status_cond)

            deadline = 5.0
            while reader.matched_writers == 0 and deadline > 0:
                try:
                    waitset.wait(timeout=1.0)
                except DdsTimeout:
                    pass
                deadline -= 1.0
            assert reader.matched_writers > 0, "Discovery failed"

            # Try to take - should be empty (VOLATILE does not resend past data)
            import time
            time.sleep(0.5)
            samples = reader.take()
            assert len(samples) == 0, "VOLATILE reader should not receive past data"

    def test_history_keep_last(self, domain_id: int):
        """KEEP_LAST depth=1: only the last sample should be available."""
        from int2dds import DdsTimeout
        from int2dds.core.qos import (
            DataWriterQos, DataReaderQos, Reliability, History,
        )
        from int2dds.core.conditions import STATUS_SUBSCRIPTION_MATCHED, STATUS_DATA_AVAILABLE

        with DomainParticipant(domain_id=domain_id) as dp:
            topic = dp.create_topic("QoS_History_Topic", TestMessage)
            pub = dp.create_publisher()
            sub = dp.create_subscriber()

            writer_qos = DataWriterQos(
                reliability=Reliability("RELIABLE"),
                history=History("KEEP_LAST", depth=1),
            )
            reader_qos = DataReaderQos(
                reliability=Reliability("RELIABLE"),
                history=History("KEEP_LAST", depth=1),
            )

            writer = pub.create_datawriter(topic, qos=writer_qos)
            reader = sub.create_datareader(topic, qos=reader_qos)

            # Wait for discovery
            status_cond = reader.get_statuscondition()
            status_cond.set_enabled_statuses(STATUS_SUBSCRIPTION_MATCHED)
            waitset = WaitSet()
            waitset.attach(status_cond)

            deadline = 5.0
            while writer.matched_readers == 0 and deadline > 0:
                try:
                    waitset.wait(timeout=1.0)
                except DdsTimeout:
                    pass
                deadline -= 1.0
            assert writer.matched_readers > 0

            # Write multiple samples rapidly
            for i in range(5):
                writer.write(TestMessage(value=i, text=f"msg_{i}"))

            # Wait for data
            status_cond.set_enabled_statuses(STATUS_DATA_AVAILABLE)
            try:
                waitset.wait(timeout=5.0)
            except DdsTimeout:
                pass

            import time
            time.sleep(0.5)

            samples = reader.take()
            # With KEEP_LAST depth=1, should receive at most 1 sample (the latest)
            assert len(samples) <= 1, f"Expected at most 1 sample, got {len(samples)}"
            if len(samples) == 1:
                assert samples[0].data.value == 4  # last written value

    def test_history_keep_all(self, domain_id: int):
        """KEEP_ALL: all written samples should be available."""
        from int2dds import DdsTimeout
        from int2dds.core.qos import (
            DataWriterQos, DataReaderQos, Reliability, History,
        )
        from int2dds.core.conditions import STATUS_SUBSCRIPTION_MATCHED, STATUS_DATA_AVAILABLE

        with DomainParticipant(domain_id=domain_id) as dp:
            topic = dp.create_topic("QoS_HistoryAll_Topic", TestMessage)
            pub = dp.create_publisher()
            sub = dp.create_subscriber()

            writer_qos = DataWriterQos(
                reliability=Reliability("RELIABLE"),
                history=History("KEEP_ALL"),
            )
            reader_qos = DataReaderQos(
                reliability=Reliability("RELIABLE"),
                history=History("KEEP_ALL"),
            )

            writer = pub.create_datawriter(topic, qos=writer_qos)
            reader = sub.create_datareader(topic, qos=reader_qos)

            # Wait for discovery
            status_cond = reader.get_statuscondition()
            status_cond.set_enabled_statuses(STATUS_SUBSCRIPTION_MATCHED)
            waitset = WaitSet()
            waitset.attach(status_cond)

            deadline = 5.0
            while writer.matched_readers == 0 and deadline > 0:
                try:
                    waitset.wait(timeout=1.0)
                except DdsTimeout:
                    pass
                deadline -= 1.0
            assert writer.matched_readers > 0

            # Write multiple samples
            status_cond.set_enabled_statuses(STATUS_DATA_AVAILABLE)
            for i in range(5):
                writer.write(TestMessage(value=i, text=f"msg_{i}"))

            # Wait for all data to arrive
            import time
            time.sleep(1.0)

            samples = reader.take()
            values = [s.data.value for s in samples]
            # With KEEP_ALL, should receive all 5 samples
            assert len(samples) == 5, f"Expected 5 samples, got {len(samples)}"
            assert values == [0, 1, 2, 3, 4]

    def test_partition_mismatch(self, domain_id: int):
        """Different partitions should not communicate."""
        from int2dds.core.publisher import Publisher
        from int2dds.core.subscriber import Subscriber
        from int2dds.core.qos import PublisherQos, SubscriberQos, Partition

        with DomainParticipant(domain_id=domain_id) as dp:
            topic = dp.create_topic("QoS_Partition_Topic", TestMessage)

            pub = Publisher(dp, qos=PublisherQos(partition=Partition(names=["groupA"])))
            sub = Subscriber(dp, qos=SubscriberQos(partition=Partition(names=["groupB"])))

            writer = pub.create_datawriter(topic)
            reader = sub.create_datareader(topic)

            import time
            time.sleep(1.0)

            # Different partitions should not match
            assert writer.matched_readers == 0
            assert reader.matched_writers == 0

    def test_partition_match(self, domain_id: int):
        """Same partition should communicate."""
        from int2dds import DdsTimeout
        from int2dds.core.publisher import Publisher
        from int2dds.core.subscriber import Subscriber
        from int2dds.core.qos import PublisherQos, SubscriberQos, Partition
        from int2dds.core.conditions import STATUS_SUBSCRIPTION_MATCHED, STATUS_DATA_AVAILABLE

        with DomainParticipant(domain_id=domain_id) as dp:
            topic = dp.create_topic("QoS_PartitionOK_Topic", TestMessage)

            pub = Publisher(dp, qos=PublisherQos(partition=Partition(names=["groupA"])))
            sub = Subscriber(dp, qos=SubscriberQos(partition=Partition(names=["groupA"])))

            writer = pub.create_datawriter(topic)
            reader = sub.create_datareader(topic)

            # Wait for discovery
            status_cond = reader.get_statuscondition()
            status_cond.set_enabled_statuses(STATUS_SUBSCRIPTION_MATCHED)
            waitset = WaitSet()
            waitset.attach(status_cond)

            deadline = 5.0
            while writer.matched_readers == 0 and deadline > 0:
                try:
                    waitset.wait(timeout=1.0)
                except DdsTimeout:
                    pass
                deadline -= 1.0
            assert writer.matched_readers > 0, "Same partition should match"

            # Write and read
            status_cond.set_enabled_statuses(STATUS_DATA_AVAILABLE)
            writer.write(TestMessage(value=77, text="partition ok"))
            try:
                waitset.wait(timeout=5.0)
            except DdsTimeout:
                pass
            samples = reader.take()
            assert len(samples) > 0
            assert samples[0].data.value == 77


class TestInstance:
    """Instance management tests for keyed types."""

    def test_register_instance(self, domain_id: int):
        """register_instance should return a non-nil 16-byte handle."""
        with DomainParticipant(domain_id=domain_id) as dp:
            topic = dp.create_topic("Instance_Register_Topic", KeyedType)
            pub = dp.create_publisher()
            writer = pub.create_datawriter(topic)

            sample = KeyedType(sensor_id=1, value=10.0)
            handle = writer.register_instance(sample)
            assert len(handle) == 16
            assert handle != b'\x00' * 16  # not NIL

    def test_lookup_instance(self, domain_id: int):
        """lookup_instance should return the same handle as register_instance."""
        with DomainParticipant(domain_id=domain_id) as dp:
            topic = dp.create_topic("Instance_Lookup_Topic", KeyedType)
            pub = dp.create_publisher()
            writer = pub.create_datawriter(topic)

            sample = KeyedType(sensor_id=2, value=20.0)
            handle = writer.register_instance(sample)
            found = writer.lookup_instance(sample)
            assert found == handle

    def test_unregister_instance(self, domain_id: int):
        """unregister_instance should complete without error."""
        with DomainParticipant(domain_id=domain_id) as dp:
            topic = dp.create_topic("Instance_Unregister_Topic", KeyedType)
            pub = dp.create_publisher()
            writer = pub.create_datawriter(topic)

            sample = KeyedType(sensor_id=3, value=30.0)
            handle = writer.register_instance(sample)
            writer.unregister_instance(sample, handle)  # should not raise

    def test_dispose_valid_data_false(self, domain_id: int):
        """After dispose, reader should receive a sample with valid_data=False."""
        from int2dds import DdsTimeout
        from int2dds.core.qos import DataWriterQos, DataReaderQos, Reliability
        from int2dds.core.conditions import STATUS_SUBSCRIPTION_MATCHED, STATUS_DATA_AVAILABLE

        with DomainParticipant(domain_id=domain_id) as dp:
            topic = dp.create_topic("Instance_Dispose_Topic", KeyedType)
            pub = dp.create_publisher()
            sub = dp.create_subscriber()

            writer_qos = DataWriterQos(reliability=Reliability("RELIABLE"))
            reader_qos = DataReaderQos(reliability=Reliability("RELIABLE"))

            writer = pub.create_datawriter(topic, qos=writer_qos)
            reader = sub.create_datareader(topic, qos=reader_qos)

            # Wait for discovery
            status_cond = reader.get_statuscondition()
            status_cond.set_enabled_statuses(STATUS_SUBSCRIPTION_MATCHED)
            waitset = WaitSet()
            waitset.attach(status_cond)

            deadline = 5.0
            while writer.matched_readers == 0 and deadline > 0:
                try:
                    waitset.wait(timeout=1.0)
                except DdsTimeout:
                    pass
                deadline -= 1.0
            assert writer.matched_readers > 0

            # Write, then dispose
            sample = KeyedType(sensor_id=4, value=40.0)
            handle = writer.register_instance(sample)
            writer.write(sample)

            status_cond.set_enabled_statuses(STATUS_DATA_AVAILABLE)
            try:
                waitset.wait(timeout=5.0)
            except DdsTimeout:
                pass
            # Take the valid sample first
            samples = reader.take()
            assert len(samples) > 0
            assert samples[0].valid_data

            # Now dispose
            writer.dispose(sample, handle)
            try:
                waitset.wait(timeout=5.0)
            except DdsTimeout:
                pass

            samples = reader.take()
            assert len(samples) > 0
            assert not samples[0].valid_data  # disposed = invalid data


class TestCommunication:
    """Tests for pub/sub communication patterns"""

    def _setup_pubsub(self, dp, topic_name, type_class):
        """Helper: create matched writer/reader pair and wait for discovery."""
        from int2dds import DdsTimeout
        from int2dds.core.qos import DataWriterQos, DataReaderQos, Reliability
        from int2dds.core.conditions import STATUS_SUBSCRIPTION_MATCHED

        topic = dp.create_topic(topic_name, type_class)
        pub = dp.create_publisher()
        sub = dp.create_subscriber()

        writer_qos = DataWriterQos(reliability=Reliability("RELIABLE"))
        reader_qos = DataReaderQos(reliability=Reliability("RELIABLE"))

        writer = pub.create_datawriter(topic, qos=writer_qos)
        reader = sub.create_datareader(topic, qos=reader_qos)

        status_cond = reader.get_statuscondition()
        status_cond.set_enabled_statuses(STATUS_SUBSCRIPTION_MATCHED)
        waitset = WaitSet()
        waitset.attach(status_cond)

        deadline = 5.0
        while writer.matched_readers == 0 and deadline > 0:
            try:
                waitset.wait(timeout=1.0)
            except DdsTimeout:
                pass
            deadline -= 1.0
        assert writer.matched_readers > 0, "Discovery failed"

        return writer, reader, waitset, status_cond

    def test_read_returns_data(self, domain_id: int):
        """read() should return written data via FFI."""
        from int2dds import DdsTimeout
        from int2dds.core.conditions import STATUS_DATA_AVAILABLE

        with DomainParticipant(domain_id=domain_id) as dp:
            writer, reader, waitset, status_cond = self._setup_pubsub(
                dp, "ReadTopic", TestMessage
            )
            status_cond.set_enabled_statuses(STATUS_DATA_AVAILABLE)

            writer.write(TestMessage(value=42, text="read test"))
            try:
                waitset.wait(timeout=5.0)
            except DdsTimeout:
                pass

            # read() returns the data correctly
            samples = reader.read()
            assert len(samples) > 0
            assert samples[0].data.value == 42
            assert samples[0].data.text == "read test"

    def test_multi_topic(self, domain_id: int):
        """Two topics should carry independent data without cross-talk."""
        from int2dds import DdsTimeout
        from int2dds.core.conditions import STATUS_DATA_AVAILABLE

        with DomainParticipant(domain_id=domain_id) as dp:
            writer_a, reader_a, ws_a, sc_a = self._setup_pubsub(
                dp, "MultiTopicA", TestMessage
            )
            writer_b, reader_b, ws_b, sc_b = self._setup_pubsub(
                dp, "MultiTopicB", TestMessage
            )
            sc_a.set_enabled_statuses(STATUS_DATA_AVAILABLE)
            sc_b.set_enabled_statuses(STATUS_DATA_AVAILABLE)

            # Write different values to each topic
            writer_a.write(TestMessage(value=1, text="topic A"))
            writer_b.write(TestMessage(value=2, text="topic B"))

            try:
                ws_a.wait(timeout=5.0)
            except DdsTimeout:
                pass
            try:
                ws_b.wait(timeout=5.0)
            except DdsTimeout:
                pass

            samples_a = reader_a.take()
            samples_b = reader_b.take()

            assert len(samples_a) > 0
            assert len(samples_b) > 0
            assert samples_a[0].data.value == 1  # TopicA data
            assert samples_b[0].data.value == 2  # TopicB data


class TestWaitSetAdvanced:
    """Tests for WaitSet advanced patterns"""

    def test_guard_condition_trigger(self):
        """GuardCondition trigger should wake up WaitSet."""
        from int2dds.core.conditions import GuardCondition

        guard = GuardCondition()
        waitset = WaitSet()
        waitset.attach(guard)

        # Before trigger: value is False
        assert guard.trigger_value is False

        # Trigger from "another thread" (same thread for test simplicity)
        guard.trigger()
        assert guard.trigger_value is True

        # WaitSet should return immediately (condition already triggered)
        waitset.wait(timeout=1.0)  # should not raise DdsTimeout

        # Reset and verify
        guard.reset()
        assert guard.trigger_value is False

        waitset.close()
        guard.close()

    def test_multi_condition_wait_ex(self, domain_id: int):
        """wait_ex() should return which conditions were triggered."""
        from int2dds.core.conditions import GuardCondition, STATUS_DATA_AVAILABLE

        with DomainParticipant(domain_id=domain_id) as dp:
            topic = dp.create_topic("MultiCondTopic", TestMessage)
            sub = dp.create_subscriber()

            from int2dds.core.qos import DataReaderQos, Reliability
            reader = sub.create_datareader(
                topic, qos=DataReaderQos(reliability=Reliability("RELIABLE"))
            )

            # Create two conditions: GuardCondition + StatusCondition
            guard = GuardCondition()
            status_cond = reader.get_statuscondition()
            status_cond.set_enabled_statuses(STATUS_DATA_AVAILABLE)

            waitset = WaitSet()
            waitset.attach(guard)
            waitset.attach(status_cond)

            # Trigger guard condition only
            guard.trigger()

            # wait_ex returns list of triggered conditions
            triggered = waitset.wait_ex(timeout=1.0)
            assert len(triggered) > 0  # at least guard was triggered

            guard.close()
            waitset.close()


class TestDiscovery:
    """Tests for discovery matching."""

    def test_matched_total_count_increases(self, domain_id: int):
        """Creating a second writer should increase reader's total matched count."""
        from int2dds import DdsTimeout
        from int2dds.core.conditions import STATUS_SUBSCRIPTION_MATCHED

        with DomainParticipant(domain_id=domain_id) as dp:
            topic = dp.create_topic("DiscoveryCountTopic", TestMessage)
            pub = dp.create_publisher()
            sub = dp.create_subscriber()

            from int2dds.core.qos import DataWriterQos, DataReaderQos, Reliability
            reader = sub.create_datareader(
                topic, qos=DataReaderQos(reliability=Reliability("RELIABLE"))
            )

            status_cond = reader.get_statuscondition()
            status_cond.set_enabled_statuses(STATUS_SUBSCRIPTION_MATCHED)
            waitset = WaitSet()
            waitset.attach(status_cond)

            # Create first writer and wait for match
            writer1 = pub.create_datawriter(
                topic, qos=DataWriterQos(reliability=Reliability("RELIABLE"))
            )
            deadline = 5.0
            while reader.matched_writers == 0 and deadline > 0:
                try:
                    waitset.wait(timeout=1.0)
                except DdsTimeout:
                    pass
                deadline -= 1.0

            total1, current1 = reader.get_subscription_matched_status()
            assert current1 >= 1

            # Create second writer and wait for match
            writer2 = pub.create_datawriter(
                topic, qos=DataWriterQos(reliability=Reliability("RELIABLE"))
            )
            deadline = 5.0
            prev_total = total1
            while True:
                try:
                    waitset.wait(timeout=1.0)
                except DdsTimeout:
                    pass
                total2, current2 = reader.get_subscription_matched_status()
                if total2 > prev_total or deadline <= 0:
                    break
                deadline -= 1.0

            assert total2 > total1  # total count increased
            assert current2 >= 2    # two writers now matched

            waitset.close()
    @pytest.mark.skipif(
    os.getenv("GITHUB_ACTIONS") == "true" and os.getenv("RUNNER_OS") == "macOS",
    reason="Skipped on GitHub Actions macOS runner: cross-participant discovery is unreliable in hosted CI",
    )
    def test_matched_decreases_on_cross_participant_delete(self, domain_id: int):
        """Deleting a writer on a different participant should decrease matched count."""
        from int2dds import DdsTimeout
        from int2dds.core.conditions import STATUS_SUBSCRIPTION_MATCHED
        from int2dds.core.qos import DataWriterQos, DataReaderQos, Reliability

        with DomainParticipant(domain_id=domain_id) as dp_writer:
            with DomainParticipant(domain_id=domain_id) as dp_reader:
                topic_w = dp_writer.create_topic("CrossParticipantTopic", TestMessage)
                topic_r = dp_reader.create_topic("CrossParticipantTopic", TestMessage)

                pub = dp_writer.create_publisher()
                sub = dp_reader.create_subscriber()

                writer = pub.create_datawriter(
                    topic_w, qos=DataWriterQos(reliability=Reliability("RELIABLE"))
                )
                reader = sub.create_datareader(
                    topic_r, qos=DataReaderQos(reliability=Reliability("RELIABLE"))
                )

                # Wait for cross-participant discovery
                status_cond = reader.get_statuscondition()
                status_cond.set_enabled_statuses(STATUS_SUBSCRIPTION_MATCHED)
                waitset = WaitSet()
                waitset.attach(status_cond)

                deadline = 10.0
                while reader.matched_writers == 0 and deadline > 0:
                    try:
                        waitset.wait(timeout=1.0)
                    except DdsTimeout:
                        pass
                    deadline -= 1.0
                assert reader.matched_writers > 0

                # Delete writer on dp_writer
                writer.close()

                # Wait for unmatch via SEDP termination message
                deadline = 10.0
                while reader.matched_writers > 0 and deadline > 0:
                    try:
                        waitset.wait(timeout=1.0)
                    except DdsTimeout:
                        pass
                    deadline -= 1.0

                assert reader.matched_writers == 0

                waitset.close()


class TestListener:
    """Tests for Listener callbacks."""

    def test_writer_on_publication_matched(self, domain_id: int):
        """DataWriterListener.on_publication_matched should be called on discovery."""
        import threading
        from int2dds.core.listeners import DataWriterListenerBase, STATUS_PUBLICATION_MATCHED
        from int2dds.core.qos import DataWriterQos, DataReaderQos, Reliability

        matched_event = threading.Event()
        received_status = {}

        class WriterListener(DataWriterListenerBase):
            def on_publication_matched(self, writer, status):
                received_status["total_count"] = status.total_count
                received_status["current_count"] = status.current_count
                matched_event.set()

        with DomainParticipant(domain_id=domain_id) as dp:
            topic = dp.create_topic("WriterListenerTopic", TestMessage)
            pub = dp.create_publisher()
            sub = dp.create_subscriber()

            listener = WriterListener()
            writer = pub.create_datawriter(
                topic,
                qos=DataWriterQos(reliability=Reliability("RELIABLE")),
                listener=listener,
                status_mask=STATUS_PUBLICATION_MATCHED,
            )

            # Create reader to trigger matching
            reader = sub.create_datareader(
                topic, qos=DataReaderQos(reliability=Reliability("RELIABLE"))
            )

            # Wait for callback
            assert matched_event.wait(timeout=10.0), "on_publication_matched not called"
            assert received_status["current_count"] >= 1

    def test_reader_on_subscription_matched(self, domain_id: int):
        """DataReaderListener.on_subscription_matched should be called on discovery."""
        import threading
        from int2dds.core.listeners import DataReaderListenerBase, STATUS_SUBSCRIPTION_MATCHED
        from int2dds.core.qos import DataWriterQos, DataReaderQos, Reliability

        matched_event = threading.Event()
        received_status = {}

        class ReaderListener(DataReaderListenerBase):
            def on_subscription_matched(self, reader, status):
                received_status["total_count"] = status.total_count
                received_status["current_count"] = status.current_count
                matched_event.set()

        with DomainParticipant(domain_id=domain_id) as dp:
            topic = dp.create_topic("ReaderListenerTopic", TestMessage)
            pub = dp.create_publisher()
            sub = dp.create_subscriber()

            listener = ReaderListener()
            reader = sub.create_datareader(
                topic,
                qos=DataReaderQos(reliability=Reliability("RELIABLE")),
                listener=listener,
                status_mask=STATUS_SUBSCRIPTION_MATCHED,
            )

            # Create writer to trigger matching
            writer = pub.create_datawriter(
                topic, qos=DataWriterQos(reliability=Reliability("RELIABLE"))
            )

            # Wait for callback
            assert matched_event.wait(timeout=5.0), "on_subscription_matched not called"
            assert received_status["current_count"] >= 1

    def test_reader_on_data_available(self, domain_id: int):
        """DataReaderListener.on_data_available should be called when data arrives."""
        import threading
        from int2dds import DdsTimeout
        from int2dds.core.listeners import DataReaderListenerBase, STATUS_DATA_AVAILABLE
        from int2dds.core.conditions import STATUS_SUBSCRIPTION_MATCHED
        from int2dds.core.qos import DataWriterQos, DataReaderQos, Reliability

        data_event = threading.Event()

        class ReaderListener(DataReaderListenerBase):
            def on_data_available(self, reader):
                data_event.set()

        with DomainParticipant(domain_id=domain_id) as dp:
            topic = dp.create_topic("DataAvailableListenerTopic", TestMessage)
            pub = dp.create_publisher()
            sub = dp.create_subscriber()

            listener = ReaderListener()
            reader = sub.create_datareader(
                topic,
                qos=DataReaderQos(reliability=Reliability("RELIABLE")),
                listener=listener,
                status_mask=STATUS_DATA_AVAILABLE,
            )

            writer = pub.create_datawriter(
                topic, qos=DataWriterQos(reliability=Reliability("RELIABLE"))
            )

            # Wait for discovery first via polling
            deadline = 5.0
            while writer.matched_readers == 0 and deadline > 0:
                import time
                time.sleep(0.5)
                deadline -= 0.5
            assert writer.matched_readers > 0

            # Write data
            writer.write(TestMessage(value=99, text="listener test"))

            # Wait for on_data_available callback
            assert data_event.wait(timeout=5.0), "on_data_available not called"

    def test_reader_set_and_remove_listener(self, domain_id: int):
        """Setting and removing a listener should work without errors."""
        import threading
        from int2dds.core.listeners import (
            DataReaderListenerBase,
            STATUS_SUBSCRIPTION_MATCHED,
            STATUS_MASK_NONE,
        )
        from int2dds.core.qos import DataWriterQos, DataReaderQos, Reliability

        first_event = threading.Event()
        second_event = threading.Event()

        class FirstListener(DataReaderListenerBase):
            def on_subscription_matched(self, reader, status):
                first_event.set()

        class SecondListener(DataReaderListenerBase):
            def on_subscription_matched(self, reader, status):
                second_event.set()

        with DomainParticipant(domain_id=domain_id) as dp:
            topic = dp.create_topic("SetListenerTopic", TestMessage)
            pub = dp.create_publisher()
            sub = dp.create_subscriber()

            # Create reader with first listener
            reader = sub.create_datareader(
                topic,
                qos=DataReaderQos(reliability=Reliability("RELIABLE")),
                listener=FirstListener(),
                status_mask=STATUS_SUBSCRIPTION_MATCHED,
            )

            # Trigger matching to call first listener
            writer = pub.create_datawriter(
                topic, qos=DataWriterQos(reliability=Reliability("RELIABLE"))
            )
            assert first_event.wait(timeout=5.0), "First listener not called"

            # Remove listener (set to None)
            reader.set_listener(None, STATUS_MASK_NONE)

            # Set second listener
            reader.set_listener(SecondListener(), STATUS_SUBSCRIPTION_MATCHED)


class TestEdgeCases:
    """Tests for edge cases and boundary values."""

    def _roundtrip(self, dp, type_class, sample, topic_name):
        """Helper: write a sample, take it back, return the result."""
        from int2dds import DdsTimeout
        from int2dds.core.qos import DataWriterQos, DataReaderQos, Reliability
        from int2dds.core.conditions import STATUS_SUBSCRIPTION_MATCHED, STATUS_DATA_AVAILABLE

        topic = dp.create_topic(topic_name, type_class)
        pub = dp.create_publisher()
        sub = dp.create_subscriber()

        writer_qos = DataWriterQos(reliability=Reliability("RELIABLE"))
        reader_qos = DataReaderQos(reliability=Reliability("RELIABLE"))

        writer = pub.create_datawriter(topic, qos=writer_qos)
        reader = sub.create_datareader(topic, qos=reader_qos)

        status_cond = reader.get_statuscondition()
        status_cond.set_enabled_statuses(STATUS_SUBSCRIPTION_MATCHED)
        waitset = WaitSet()
        waitset.attach(status_cond)

        deadline = 5.0
        while writer.matched_readers == 0 and deadline > 0:
            try:
                waitset.wait(timeout=1.0)
            except DdsTimeout:
                pass
            deadline -= 1.0
        assert writer.matched_readers > 0, "Discovery failed"

        status_cond.set_enabled_statuses(STATUS_DATA_AVAILABLE)
        writer.write(sample)

        try:
            waitset.wait(timeout=5.0)
        except DdsTimeout:
            pass

        samples = reader.take()
        assert len(samples) > 0, "No samples received"
        return samples[0].data

    def test_empty_string(self, domain_id: int):
        """Empty string should survive FFI roundtrip."""
        with DomainParticipant(domain_id=domain_id) as dp:
            original = TestMessage(value=1, text="")
            result = self._roundtrip(dp, TestMessage, original, "EmptyStringTopic")
            assert result.text == ""
            assert result.value == 1

    def test_empty_sequence(self, domain_id: int):
        """Empty sequence should survive FFI roundtrip."""
        with DomainParticipant(domain_id=domain_id) as dp:
            original = SequenceType(items=[])
            result = self._roundtrip(dp, SequenceType, original, "EmptySeqTopic")
            assert result.items == []

    def test_max_integers(self, domain_id: int):
        """Maximum integer values should survive FFI roundtrip."""
        with DomainParticipant(domain_id=domain_id) as dp:
            original = AllPrimitives(
                flag=True,
                byte_val=255,              # u8 max
                ibyte_val=-128,            # i8 min
                short_val=-32768,          # i16 min
                ushort_val=65535,          # u16 max
                long_val=-2147483648,      # i32 min
                ulong_val=4294967295,      # u32 max
                llong_val=-9223372036854775808,   # i64 min
                ullong_val=18446744073709551615,  # u64 max
                float_val=3.4028235e+38,  # f32 near max
                double_val=1.7976931348623157e+308,  # f64 near max
                char_val="Z",
                text="max values",
            )
            result = self._roundtrip(dp, AllPrimitives, original, "MaxIntTopic")
            assert result.byte_val == 255
            assert result.ibyte_val == -128
            assert result.short_val == -32768
            assert result.ushort_val == 65535
            assert result.long_val == -2147483648
            assert result.ulong_val == 4294967295
            assert result.llong_val == -9223372036854775808
            assert result.ullong_val == 18446744073709551615

    def test_large_string(self, domain_id: int):
        """10KB+ string should survive FFI roundtrip."""
        with DomainParticipant(domain_id=domain_id) as dp:
            large_text = "A" * 10240  # 10KB
            original = TestMessage(value=42, text=large_text)
            result = self._roundtrip(dp, TestMessage, original, "LargeStringTopic")
            assert result.text == large_text
            assert len(result.text) == 10240


class TestHighVolume:
    """Tests for large data and high-frequency write/take."""

    def _setup_pubsub(self, dp, topic_name, type_class):
        """Helper: create matched writer/reader pair and wait for discovery."""
        from int2dds import DdsTimeout
        from int2dds.core.qos import DataWriterQos, DataReaderQos, Reliability, History
        from int2dds.core.conditions import STATUS_SUBSCRIPTION_MATCHED

        topic = dp.create_topic(topic_name, type_class)
        pub = dp.create_publisher()
        sub = dp.create_subscriber()

        writer_qos = DataWriterQos(
            reliability=Reliability("RELIABLE"),
            history=History("KEEP_ALL"),
        )
        reader_qos = DataReaderQos(
            reliability=Reliability("RELIABLE"),
            history=History("KEEP_ALL"),
        )

        writer = pub.create_datawriter(topic, qos=writer_qos)
        reader = sub.create_datareader(topic, qos=reader_qos)

        status_cond = reader.get_statuscondition()
        status_cond.set_enabled_statuses(STATUS_SUBSCRIPTION_MATCHED)
        waitset = WaitSet()
        waitset.attach(status_cond)

        deadline = 5.0
        while writer.matched_readers == 0 and deadline > 0:
            try:
                waitset.wait(timeout=1.0)
            except DdsTimeout:
                pass
            deadline -= 1.0
        assert writer.matched_readers > 0, "Discovery failed"

        return writer, reader, waitset, status_cond

    def test_large_payload_near_buffer_limit(self, domain_id: int):
        """Payload near DEFAULT_BUFFER_SIZE (64KB) should be transmitted."""
        from int2dds import DdsTimeout
        from int2dds.core.conditions import STATUS_DATA_AVAILABLE

        with DomainParticipant(domain_id=domain_id) as dp:
            writer, reader, waitset, status_cond = self._setup_pubsub(
                dp, "LargePayloadTopic", TestMessage
            )
            status_cond.set_enabled_statuses(STATUS_DATA_AVAILABLE)

            # 60KB - within 64KB buffer limit (leaves room for CDR header + length prefix)
            large_text = "B" * (60 * 1024)
            writer.write(TestMessage(value=1, text=large_text))

            try:
                waitset.wait(timeout=10.0)
            except DdsTimeout:
                pass

            samples = reader.take()
            assert len(samples) > 0, "No samples received"
            assert len(samples[0].data.text) == 60 * 1024

    def test_100_consecutive_writes(self, domain_id: int):
        """100 consecutive writes should all be received via take."""
        import time
        from int2dds import DdsTimeout
        from int2dds.core.conditions import STATUS_DATA_AVAILABLE

        with DomainParticipant(domain_id=domain_id) as dp:
            writer, reader, waitset, status_cond = self._setup_pubsub(
                dp, "Batch100Topic", TestMessage
            )
            status_cond.set_enabled_statuses(STATUS_DATA_AVAILABLE)

            for i in range(100):
                writer.write(TestMessage(value=i, text=f"msg_{i}"))

            time.sleep(2.0)

            all_samples = []
            while True:
                samples = reader.take()
                if not samples:
                    break
                all_samples.extend(samples)

            assert len(all_samples) == 100, f"Expected 100, got {len(all_samples)}"
            values = [s.data.value for s in all_samples]
            assert values == list(range(100))

    def test_1000_consecutive_writes(self, domain_id: int):
        """1000 consecutive writes should all be received via take."""
        import time
        from int2dds import DdsTimeout
        from int2dds.core.conditions import STATUS_DATA_AVAILABLE

        with DomainParticipant(domain_id=domain_id) as dp:
            writer, reader, waitset, status_cond = self._setup_pubsub(
                dp, "Batch1000Topic", TestMessage
            )
            status_cond.set_enabled_statuses(STATUS_DATA_AVAILABLE)

            for i in range(1000):
                writer.write(TestMessage(value=i, text=f"msg_{i}"))

            time.sleep(5.0)

            all_samples = []
            while True:
                samples = reader.take()
                if not samples:
                    break
                all_samples.extend(samples)

            assert len(all_samples) == 1000, f"Expected 1000, got {len(all_samples)}"
            values = [s.data.value for s in all_samples]
            assert values == list(range(1000))


class TestResourceCleanup:
    """Tests for resource cleanup and re-creation."""

    def test_context_manager_recreate(self, domain_id: int):
        """After context manager exit, a new Participant should be creatable."""
        with DomainParticipant(domain_id=domain_id) as dp1:
            topic = dp1.create_topic("RecreateTest1", TestMessage)
            assert topic.name == "RecreateTest1"
            topic.close()

        # After dp1 is closed, create a new one
        with DomainParticipant(domain_id=domain_id) as dp2:
            topic = dp2.create_topic("RecreateTest2", TestMessage)
            assert topic.name == "RecreateTest2"
            topic.close()

    def test_participant_sequential_create_delete(self, domain_id: int):
        """Sequential create/delete cycles should not leak resources."""
        for i in range(5):
            with DomainParticipant(domain_id=domain_id) as dp:
                topic = dp.create_topic(f"SeqTest_{i}", TestMessage)
                assert topic.name == f"SeqTest_{i}"
                topic.close()

    def test_writer_reader_recreate(self, domain_id: int):
        """After deleting Writer/Reader, new ones should be creatable."""
        with DomainParticipant(domain_id=domain_id) as dp:
            topic = dp.create_topic("RecreateWRTopic", TestMessage)
            pub = dp.create_publisher()
            sub = dp.create_subscriber()

            # First creation
            writer = pub.create_datawriter(topic)
            reader = sub.create_datareader(topic)

            writer.close()
            reader.close()

            # Re-creation after delete
            writer2 = pub.create_datawriter(topic)
            reader2 = sub.create_datareader(topic)

            assert writer2 is not None
            assert reader2 is not None

            writer2.close()
            reader2.close()
            pub.close()
            sub.close()
            topic.close()


class TestErrorHandling:
    """Tests for error handling on invalid operations."""

    def test_write_on_closed_writer(self, domain_id: int):
        """Writing on a closed DataWriter should raise an exception."""
        with DomainParticipant(domain_id=domain_id) as dp:
            topic = dp.create_topic("ClosedWriterTopic", TestMessage)
            pub = dp.create_publisher()
            writer = pub.create_datawriter(topic)
            writer.close()

            with pytest.raises(Exception):
                writer.write(TestMessage(value=1, text="fail"))

            pub.close()
            topic.close()

    def test_take_on_closed_reader(self, domain_id: int):
        """Taking from a closed DataReader should raise an exception."""
        with DomainParticipant(domain_id=domain_id) as dp:
            topic = dp.create_topic("ClosedReaderTopic", TestMessage)
            sub = dp.create_subscriber()
            reader = sub.create_datareader(topic)
            reader.close()

            with pytest.raises(Exception):
                reader.take()

            sub.close()
            topic.close()

    def test_create_topic_on_closed_participant(self, domain_id: int):
        """Creating a topic on a closed Participant should raise an exception."""
        dp = DomainParticipant(domain_id=domain_id)
        dp.close()

        with pytest.raises(Exception):
            dp.create_topic("FailTopic", TestMessage)

    def test_incompatible_qos_combination(self, domain_id: int):
        """Incompatible QoS should prevent matching, not crash."""
        from int2dds.core.qos import (
            DataReaderQos,
            DataWriterQos,
            Reliability,
        )
        import time

        with DomainParticipant(domain_id=domain_id) as dp:
            topic = dp.create_topic("BadQosTopic", TestMessage)
            pub = dp.create_publisher()
            sub = dp.create_subscriber()

            writer_qos = DataWriterQos(reliability=Reliability("BEST_EFFORT"))
            reader_qos = DataReaderQos(reliability=Reliability("RELIABLE"))

            writer = pub.create_datawriter(topic, qos=writer_qos)
            reader = sub.create_datareader(topic, qos=reader_qos)

            time.sleep(1.0)

            # Should not match - no crash, just 0 matched
            assert writer.matched_readers == 0
            assert reader.matched_writers == 0

            writer.close()
            reader.close()
            pub.close()
            sub.close()
            topic.close()

    def test_none_type_write(self, domain_id: int):
        """Writing None should raise an exception."""
        with DomainParticipant(domain_id=domain_id) as dp:
            topic = dp.create_topic("NoneWriteTopic", TestMessage)
            pub = dp.create_publisher()
            writer = pub.create_datawriter(topic)

            with pytest.raises(Exception):
                writer.write(None)

            writer.close()
            pub.close()
            topic.close()

    def test_create_subscriber_missing_profile_includes_reason(self, domain_id: int):
        """DdsError raised for a missing QoS profile should carry the FFI reason."""
        from int2dds.exceptions import DdsError

        participant = DomainParticipant(domain_id=domain_id)
        try:
            with pytest.raises(DdsError) as excinfo:
                participant.create_subscriber_with_profile("NoSuchLib::NoSuchProfile")
            assert "QoS profile not found" in str(excinfo.value)
        finally:
            participant.close()
