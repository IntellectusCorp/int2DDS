"""
Tests for CDR serialization/deserialization.
"""

import pytest

from int2dds.cdr import CdrReader, CdrWriter, Extensibility


class TestCdrWriter:
    """Test CdrWriter basic functionality."""

    def test_write_bool(self):
        w = CdrWriter()
        w.write_bool(True)
        w.write_bool(False)
        data = w.to_bytes()
        # Check encapsulation header + data
        assert len(data) >= 6  # 4 byte header + 2 bytes

    def test_write_integers(self):
        w = CdrWriter()
        w.write_u8(255)
        w.write_i8(-128)
        w.write_u16(65535)
        w.write_i16(-32768)
        w.write_u32(0xFFFFFFFF)
        w.write_i32(-2147483648)
        w.write_u64(0xFFFFFFFFFFFFFFFF)
        w.write_i64(-9223372036854775808)
        data = w.to_bytes()
        assert len(data) > 4  # Has encapsulation header

    def test_write_float(self):
        w = CdrWriter()
        w.write_f32(3.14159)
        w.write_f64(2.718281828)
        data = w.to_bytes()
        assert len(data) > 4

    def test_write_string(self):
        w = CdrWriter()
        w.write_string("Hello, World!")
        data = w.to_bytes()
        assert len(data) > 4

    def test_write_empty_string(self):
        w = CdrWriter()
        w.write_string("")
        data = w.to_bytes()
        assert len(data) > 4


class TestCdrReader:
    """Test CdrReader basic functionality."""

    def test_read_bool(self):
        w = CdrWriter()
        w.write_bool(True)
        w.write_bool(False)
        data = w.to_bytes()

        r = CdrReader(data)
        assert r.read_bool() is True
        assert r.read_bool() is False

    def test_read_integers(self):
        w = CdrWriter()
        w.write_u8(200)
        w.write_i8(-100)
        w.write_u16(50000)
        w.write_i16(-20000)
        w.write_u32(3000000000)
        w.write_i32(-1500000000)
        data = w.to_bytes()

        r = CdrReader(data)
        assert r.read_u8() == 200
        assert r.read_i8() == -100
        assert r.read_u16() == 50000
        assert r.read_i16() == -20000
        assert r.read_u32() == 3000000000
        assert r.read_i32() == -1500000000

    def test_read_float(self):
        w = CdrWriter()
        w.write_f32(3.14)
        w.write_f64(2.718)
        data = w.to_bytes()

        r = CdrReader(data)
        assert abs(r.read_f32() - 3.14) < 0.01
        assert abs(r.read_f64() - 2.718) < 0.001

    def test_read_string(self):
        w = CdrWriter()
        w.write_string("Hello, World!")
        data = w.to_bytes()

        r = CdrReader(data)
        assert r.read_string() == "Hello, World!"

    def test_roundtrip_unicode(self):
        w = CdrWriter()
        original = "Hello, 世界! 🌍"
        w.write_string(original)
        data = w.to_bytes()

        r = CdrReader(data)
        assert r.read_string() == original


class TestCdrExtensibility:
    """Test XCDR2 extensibility support."""

    def test_final_extensibility(self):
        w = CdrWriter(extensibility=Extensibility.FINAL)
        w.write_u32(42)
        w.write_string("test")
        data = w.to_bytes()

        r = CdrReader(data)
        assert r.read_u32() == 42
        assert r.read_string() == "test"

    def test_appendable_extensibility(self):
        w = CdrWriter(extensibility=Extensibility.APPENDABLE, xcdr2=True)
        with w.dheader():
            w.write_u32(42)
            w.write_string("test")
        data = w.to_bytes()

        r = CdrReader(data)
        dsize, dstart = r.read_dheader()
        assert dsize > 0
        assert r.read_u32() == 42
        assert r.read_string() == "test"

    def test_mutable_extensibility(self):
        w = CdrWriter(extensibility=Extensibility.MUTABLE, xcdr2=True)
        with w.dheader():
            with w.emheader(member_id=0):
                w.write_u32(42)
            with w.emheader(member_id=1):
                w.write_string("test")
            w.write_sentinel()
        data = w.to_bytes()

        r = CdrReader(data)
        dsize, dstart = r.read_dheader()
        assert dsize > 0


class TestCdrSequence:
    """Test sequence serialization."""

    def test_write_seq_header(self):
        w = CdrWriter()
        w.write_seq_header(5)
        for i in range(5):
            w.write_u32(i)
        data = w.to_bytes()

        r = CdrReader(data)
        length = r.read_seq_header()
        assert length == 5
        for i in range(5):
            assert r.read_u32() == i

    def test_empty_sequence(self):
        w = CdrWriter()
        w.write_seq_header(0)
        data = w.to_bytes()

        r = CdrReader(data)
        assert r.read_seq_header() == 0
