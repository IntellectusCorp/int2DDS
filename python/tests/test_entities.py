"""
Tests for DDS entities (DomainParticipant, Publisher, Subscriber, etc.)

These tests require the int2dds_ffi library to be available.
Set INT2DDS_FFI_PATH environment variable if the library is not in the standard paths.
"""

from dataclasses import dataclass, field
from enum import IntEnum
from typing import ClassVar

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

    def _serialize_cdr(self) -> bytes:
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

    def _serialize_cdr(self) -> bytes:
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

    def _serialize_cdr(self) -> bytes:
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

    def _serialize_cdr(self) -> bytes:
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

    def _serialize_cdr(self) -> bytes:
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

    def _serialize_cdr(self) -> bytes:
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

    def _serialize_cdr(self) -> bytes:
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

    def _serialize_cdr(self) -> bytes:
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

    def _serialize_cdr(self) -> bytes:
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


class TestRoundtrip:
    """Write -> Take roundtrip tests for FFI interop verification."""

    def _roundtrip(self, dp, type_class, sample, topic_name):
        """Helper: write a sample, take it back, return the result."""
        from int2dds import DdsTimeout
        from int2dds.core.conditions import STATUS_DATA_AVAILABLE, STATUS_SUBSCRIPTION_MATCHED

        topic = dp.create_topic(topic_name, type_class)
        pub = dp.create_publisher()
        sub = dp.create_subscriber()
        writer = pub.create_datawriter(topic)
        reader = sub.create_datareader(topic)

        # Wait for discovery (writer <-> reader matching)
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
        assert writer.matched_readers > 0, "No matched readers (discovery failed)"

        # Switch to DATA_AVAILABLE and write
        status_cond.set_enabled_statuses(STATUS_DATA_AVAILABLE)
        writer.write(sample)

        # Wait for data and take
        try:
            waitset.wait(timeout=5.0)
        except DdsTimeout:
            pass

        samples = reader.take()
        assert len(samples) > 0, "No samples received"
        assert samples[0].valid_data, "Sample has invalid data"
        return samples[0].data

    def test_all_primitives(self, domain_id: int):
        with DomainParticipant(domain_id=domain_id) as dp:
            original = AllPrimitives(
                flag=True,
                byte_val=255,
                ibyte_val=-128,
                short_val=-1000,
                ushort_val=60000,
                long_val=-100000,
                ulong_val=3000000000,
                llong_val=-9000000000000,
                ullong_val=18000000000000000000,
                float_val=3.14,
                double_val=2.718281828,
                char_val="A",
                text="hello primitives",
            )
            result = self._roundtrip(dp, AllPrimitives, original, "AllPrimitivesTopic")
            assert result.flag == original.flag
            assert result.byte_val == original.byte_val
            assert result.ibyte_val == original.ibyte_val
            assert result.short_val == original.short_val
            assert result.ushort_val == original.ushort_val
            assert result.long_val == original.long_val
            assert result.ulong_val == original.ulong_val
            assert result.llong_val == original.llong_val
            assert result.ullong_val == original.ullong_val
            assert abs(result.float_val - original.float_val) < 1e-5
            assert abs(result.double_val - original.double_val) < 1e-10
            assert result.char_val == original.char_val
            assert result.text == original.text

    def test_array_type(self, domain_id: int):
        with DomainParticipant(domain_id=domain_id) as dp:
            original = ArrayType(values=[10, 20, 30, 40, 50])
            result = self._roundtrip(dp, ArrayType, original, "ArrayTopic")
            assert result.values == original.values

    def test_sequence_type(self, domain_id: int):
        with DomainParticipant(domain_id=domain_id) as dp:
            original = SequenceType(items=[100, 200, 300])
            result = self._roundtrip(dp, SequenceType, original, "SequenceTopic")
            assert result.items == original.items

    def test_enum_message(self, domain_id: int):
        with DomainParticipant(domain_id=domain_id) as dp:
            original = EnumMessage(id=42, color=Color.BLUE, status=StatusKind.ACTIVE)
            result = self._roundtrip(dp, EnumMessage, original, "EnumTopic")
            assert result.id == original.id
            assert result.color == original.color
            assert result.status == original.status

    def test_nested_type(self, domain_id: int):
        with DomainParticipant(domain_id=domain_id) as dp:
            original = NestedType(
                id=99,
                inner=InnerStruct(x=10, y=20, name="nested"),
            )
            result = self._roundtrip(dp, NestedType, original, "NestedTopic")
            assert result.id == original.id
            assert result.inner.x == original.inner.x
            assert result.inner.y == original.inner.y
            assert result.inner.name == original.inner.name

    def test_keyed_type(self, domain_id: int):
        with DomainParticipant(domain_id=domain_id) as dp:
            original = KeyedType(sensor_id=7, value=25.5)
            result = self._roundtrip(dp, KeyedType, original, "KeyedTopic_RT")
            assert result.sensor_id == original.sensor_id
            assert abs(result.value - original.value) < 1e-10

    def test_appendable_msg(self, domain_id: int):
        with DomainParticipant(domain_id=domain_id) as dp:
            original = AppendableMsg(id=1, value=99.9)
            result = self._roundtrip(dp, AppendableMsg, original, "AppendableTopic")
            assert result.id == original.id
            assert abs(result.value - original.value) < 1e-10

    def test_mutable_msg(self, domain_id: int):
        with DomainParticipant(domain_id=domain_id) as dp:
            original = MutableMsg(id=2, value=77.7)
            result = self._roundtrip(dp, MutableMsg, original, "MutableTopic")
            assert result.id == original.id
            assert abs(result.value - original.value) < 1e-10
