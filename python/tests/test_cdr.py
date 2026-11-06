"""
Tests for CDR serialization/deserialization.
"""

import array
import struct
import sys

import pytest

from int2dds.cdr import CdrReader, CdrWriter, Extensibility
from int2dds.cdr.reader import CdrUnderflowError


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


# struct format character -> sample values
_PRIM_CASES = {
    "?": [True, False, True],
    "b": [-128, 0, 127],
    "B": [0, 1, 255],
    "h": [-32768, 0, 32767],
    "H": [0, 1, 65535],
    "i": [-(2**31), 0, 2**31 - 1],
    "I": [0, 1, 2**32 - 1],
    "q": [-(2**63), 0, 2**63 - 1],
    "Q": [0, 1, 2**64 - 1],
    "f": [-1.5, 0.0, 3.25],
    "d": [-1.5, 0.0, 1e300],
}

_ENCODINGS = [
    pytest.param(True, False, id="xcdr1-le"),
    pytest.param(False, False, id="xcdr1-be"),
    pytest.param(True, True, id="xcdr2-le"),
    pytest.param(False, True, id="xcdr2-be"),
]


class TestCdrPrimitiveSequence:
    """Test bulk primitive sequence/array serialization."""

    @pytest.mark.parametrize(
        "xcdr2, expected",
        [
            pytest.param(
                False,
                b"\x00\x00\x00\x01"  # u32 1
                b"\x07\x00\x00\x00"  # u8 7 + padding
                b"\x00\x00\x00\x01"  # count 1
                b"\x00\x00\x00\x00"  # padding to 8
                b"\x3f\xf0\x00\x00\x00\x00\x00\x00",  # 1.0
                id="xcdr1",
            ),
            pytest.param(
                True,
                b"\x00\x00\x00\x01"  # u32 1
                b"\x07\x00\x00\x00"  # u8 7 + padding
                b"\x00\x00\x00\x01"  # count 1
                b"\x3f\xf0\x00\x00\x00\x00\x00\x00",  # 1.0, alignment capped at 4
                id="xcdr2",
            ),
        ],
    )
    def test_prim_seq_double_wire_big_endian(self, xcdr2, expected):
        w = CdrWriter(little_endian=False, xcdr2=xcdr2)
        w.write_u32(1)
        w.write_u8(7)
        w.write_prim_seq([1.0], "d")

        assert w.to_bytes()[4:] == expected

    def test_prim_seq_short_wire_little_endian(self):
        w = CdrWriter(little_endian=True)
        w.write_prim_seq([1, -2], "h")

        assert w.to_bytes()[4:] == b"\x02\x00\x00\x00\x01\x00\xfe\xff"

    def test_prim_seq_bool_wire(self):
        w = CdrWriter(little_endian=True)
        w.write_prim_seq([True, False, 2], "?")

        assert w.to_bytes()[4:] == b"\x03\x00\x00\x00\x01\x00\x01"

        r = CdrReader.from_raw(b"\x03\x00\x00\x00\x01\x00\x07")
        assert r.read_prim_seq("?") == [True, False, True]

    @pytest.mark.parametrize(
        "fmt_char, values, xcdr2, expected",
        [
            pytest.param("h", [1], False, b"\x07\x00\x01\x00", id="short"),
            pytest.param("i", [1], False, b"\x07" + b"\x00" * 3 + b"\x01\x00\x00\x00", id="long"),
            pytest.param(
                "d",
                [1.0],
                False,
                b"\x07" + b"\x00" * 7 + b"\x00" * 6 + b"\xf0\x3f",
                id="double-xcdr1",
            ),
            pytest.param(
                "d",
                [1.0],
                True,
                b"\x07" + b"\x00" * 3 + b"\x00" * 6 + b"\xf0\x3f",
                id="double-xcdr2",
            ),
        ],
    )
    def test_prim_array_wire_padding(self, fmt_char, values, xcdr2, expected):
        w = CdrWriter(little_endian=True, xcdr2=xcdr2)
        w.write_u8(7)
        w.write_prim_array(values, fmt_char, len(values))

        assert w.to_bytes()[4:] == expected

    @pytest.mark.parametrize("little_endian, xcdr2", _ENCODINGS)
    @pytest.mark.parametrize("fmt_char", list(_PRIM_CASES))
    def test_prim_seq_roundtrip(self, fmt_char, little_endian, xcdr2):
        values = _PRIM_CASES[fmt_char]

        w = CdrWriter(little_endian=little_endian, xcdr2=xcdr2)
        w.write_u32(1)
        w.write_u8(7)
        w.write_prim_seq(values, fmt_char)
        w.write_u8(8)
        w.write_prim_array(values, fmt_char, len(values))
        w.write_u8(9)

        r = CdrReader(w.to_bytes())
        assert r.read_u32() == 1
        assert r.read_u8() == 7
        assert r.read_prim_seq(fmt_char) == values
        assert r.read_u8() == 8
        assert r.read_prim_array(fmt_char, len(values)) == values
        assert r.read_u8() == 9
        assert r.remaining == 0

    @pytest.mark.parametrize(
        "values, fmt_char, is_copied",
        [
            pytest.param(array.array("f", [1.0, -2.5]), "f", True, id="array-f"),
            pytest.param(array.array("q", [1, -2]), "q", True, id="array-q"),
            pytest.param(b"\x01\xff", "B", True, id="bytes"),
            pytest.param(bytearray(b"\x01\xff"), "B", True, id="bytearray"),
            pytest.param(array.array("d", [1.0, -2.5]), "f", False, id="size-mismatch"),
            pytest.param(array.array("i", [1, 2]), "I", False, id="kind-mismatch"),
            pytest.param(
                memoryview(array.array("d", [1.0, 2.0, 3.0]))[::2], "d", False, id="strided"
            ),
            pytest.param(b"\x00\x02", "?", False, id="bytes-as-bool"),
        ],
    )
    @pytest.mark.parametrize("little_endian", [True, False], ids=["le", "be"])
    def test_prim_seq_buffer_matches_list(self, values, fmt_char, is_copied, little_endian):
        expected = CdrWriter(little_endian=little_endian)
        expected.write_u8(7)
        expected.write_prim_seq(list(values), fmt_char)

        w = CdrWriter(little_endian=little_endian)
        w.write_u8(7)
        w.write_prim_seq(values, fmt_char)

        assert w.to_bytes() == expected.to_bytes()

        is_host_order = little_endian == (sys.byteorder == "little")
        size = struct.calcsize("<" + fmt_char)
        view = w._wire_layout_view(values, fmt_char, size)
        assert (view is not None) == (is_copied and (is_host_order or size == 1))

    def test_prim_array_buffer_size_mismatch_raises(self):
        w = CdrWriter()

        with pytest.raises(struct.error):
            w.write_prim_array(array.array("f", [1.0, 2.0]), "f", 3)

    def test_empty_prim_seq_writes_no_padding(self):
        w = CdrWriter(little_endian=True, xcdr2=False)
        w.write_prim_seq([], "d")
        w.write_u8(7)

        assert w.to_bytes()[4:] == b"\x00\x00\x00\x00\x07"

        r = CdrReader(w.to_bytes())
        assert r.read_prim_seq("d") == []
        assert r.read_u8() == 7

    def test_prim_array_size_mismatch_raises(self):
        w = CdrWriter()

        with pytest.raises(struct.error):
            w.write_prim_array([1.0, 2.0], "f", 3)

    def test_prim_u8_out_of_range_raises(self):
        w = CdrWriter()

        with pytest.raises(struct.error):
            w.write_prim_seq([256], "B")

    def test_prim_seq_underflow_raises(self):
        w = CdrWriter()
        w.write_seq_header(4)
        w.write_f64(1.0)

        r = CdrReader(w.to_bytes())
        with pytest.raises(CdrUnderflowError):
            r.read_prim_seq("d")
