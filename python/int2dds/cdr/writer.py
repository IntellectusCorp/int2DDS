"""
CDR/XCDR2 Writer for serializing DDS data types.
"""

from __future__ import annotations

import struct
from contextlib import contextmanager
from enum import IntEnum
from typing import TYPE_CHECKING, Iterator

if TYPE_CHECKING:
    pass


class Extensibility(IntEnum):
    """DDS extensibility kinds for type encoding."""

    FINAL = 0
    APPENDABLE = 1
    MUTABLE = 2


# Encapsulation IDs (big-endian in header)
_ENCAP_CDR_BE = 0x0000
_ENCAP_CDR_LE = 0x0001
_ENCAP_CDR2_BE = 0x0006  # PLAINCDR2 BE (Final)
_ENCAP_CDR2_LE = 0x0007  # PLAINCDR2 LE (Final)
_ENCAP_DCDR2_BE = 0x0008  # DELIMITED_CDR2 BE (Appendable)
_ENCAP_DCDR2_LE = 0x0009  # DELIMITED_CDR2 LE (Appendable)
_ENCAP_PL_CDR2_BE = 0x000A  # PL_CDR2 BE (Mutable)
_ENCAP_PL_CDR2_LE = 0x000B  # PL_CDR2 LE (Mutable)

# Sentinel for mutable types
MEMBER_ID_SENTINEL = 0x3F02


class CdrWriter:
    """
    CDR/XCDR2 serialization writer.

    Writes data in CDR (Common Data Representation) format with optional
    XCDR2 extensions for appendable and mutable types.

    Example:
        >>> writer = CdrWriter(extensibility=Extensibility.APPENDABLE)
        >>> writer.write_u32(42)
        >>> writer.write_string("hello")
        >>> data = writer.to_bytes()
    """

    __slots__ = ("_buf", "_le", "_xcdr2", "_header_size", "_extensibility")

    def __init__(
        self,
        extensibility: Extensibility = Extensibility.APPENDABLE,
        little_endian: bool = True,
        xcdr2: bool = False,
    ) -> None:
        """
        Initialize a CDR writer.

        Args:
            extensibility: Type extensibility (FINAL, APPENDABLE, MUTABLE)
            little_endian: Use little-endian byte order (default True)
            xcdr2: Use XCDR2 encoding (default False; effective write default is XCDR1)
        """
        self._buf = bytearray()
        self._le = little_endian
        self._xcdr2 = xcdr2
        self._extensibility = extensibility
        self._header_size = 0

        # Write encapsulation header
        self._write_encapsulation()

    def _write_encapsulation(self) -> None:
        """Write the CDR encapsulation header."""
        if self._xcdr2:
            if self._extensibility == Extensibility.FINAL:
                encap_id = _ENCAP_CDR2_LE if self._le else _ENCAP_CDR2_BE
            elif self._extensibility == Extensibility.APPENDABLE:
                encap_id = _ENCAP_DCDR2_LE if self._le else _ENCAP_DCDR2_BE
            else:  # MUTABLE
                encap_id = _ENCAP_PL_CDR2_LE if self._le else _ENCAP_PL_CDR2_BE
        else:
            encap_id = _ENCAP_CDR_LE if self._le else _ENCAP_CDR_BE

        # Encapsulation header is always big-endian: [encap_id(2), options(2)]
        self._buf.extend(struct.pack(">HH", encap_id, 0))
        self._header_size = 4

    def _align(self, alignment: int) -> None:
        """Align the write position to the given boundary."""
        if alignment <= 1:
            return

        # XCDR2: max alignment capped at 4 bytes
        actual = min(alignment, 4) if self._xcdr2 else alignment

        # Alignment is relative to data start (after encapsulation header)
        stream_pos = len(self._buf) - self._header_size
        padding = (actual - (stream_pos % actual)) % actual

        if padding > 0:
            self._buf.extend(b"\x00" * padding)

    # -------------------------------------------------------------------------
    # Primitive writes
    # -------------------------------------------------------------------------

    def write_bool(self, val: bool) -> None:
        """Write a boolean value (1 byte)."""
        self._buf.append(1 if val else 0)

    def write_u8(self, val: int) -> None:
        """Write an unsigned 8-bit integer."""
        self._buf.append(val & 0xFF)

    def write_i8(self, val: int) -> None:
        """Write a signed 8-bit integer."""
        self._buf.extend(struct.pack("b", val))

    def write_u16(self, val: int) -> None:
        """Write an unsigned 16-bit integer."""
        self._align(2)
        fmt = "<H" if self._le else ">H"
        self._buf.extend(struct.pack(fmt, val))

    def write_i16(self, val: int) -> None:
        """Write a signed 16-bit integer."""
        self._align(2)
        fmt = "<h" if self._le else ">h"
        self._buf.extend(struct.pack(fmt, val))

    def write_u32(self, val: int) -> None:
        """Write an unsigned 32-bit integer."""
        self._align(4)
        fmt = "<I" if self._le else ">I"
        self._buf.extend(struct.pack(fmt, val))

    def write_i32(self, val: int) -> None:
        """Write a signed 32-bit integer."""
        self._align(4)
        fmt = "<i" if self._le else ">i"
        self._buf.extend(struct.pack(fmt, val))

    def write_u64(self, val: int) -> None:
        """Write an unsigned 64-bit integer."""
        self._align(4 if self._xcdr2 else 8)  # XCDR2 caps at 4-byte alignment
        fmt = "<Q" if self._le else ">Q"
        self._buf.extend(struct.pack(fmt, val))

    def write_i64(self, val: int) -> None:
        """Write a signed 64-bit integer."""
        self._align(4 if self._xcdr2 else 8)
        fmt = "<q" if self._le else ">q"
        self._buf.extend(struct.pack(fmt, val))

    def write_f32(self, val: float) -> None:
        """Write a 32-bit float."""
        self._align(4)
        fmt = "<f" if self._le else ">f"
        self._buf.extend(struct.pack(fmt, val))

    def write_f64(self, val: float) -> None:
        """Write a 64-bit double."""
        self._align(4 if self._xcdr2 else 8)
        fmt = "<d" if self._le else ">d"
        self._buf.extend(struct.pack(fmt, val))

    def write_char(self, val: str) -> None:
        """Write a single character (1 byte)."""
        self._buf.append(ord(val[0]) if val else 0)

    # -------------------------------------------------------------------------
    # String and sequence
    # -------------------------------------------------------------------------

    def write_string(self, val: str) -> None:
        """
        Write a string with length prefix.

        The string is encoded as UTF-8 with a null terminator.
        Length includes the null terminator.
        """
        encoded = val.encode("utf-8") + b"\x00"
        self.write_u32(len(encoded))
        self._buf.extend(encoded)

    def write_wstring(self, val: str) -> None:
        """
        Write a wide string (UTF-16) with length prefix.

        Length is the number of UTF-16 code units (NOT bytes, NOT including null).
        No null terminator is written.
        """
        # Encode as UTF-16 without BOM
        encoding = "utf-16-le" if self._le else "utf-16-be"
        encoded = val.encode(encoding)
        # Number of UTF-16 code units = bytes / 2
        code_units = len(encoded) // 2
        self.write_u32(code_units)
        self._align(2)
        self._buf.extend(encoded)

    def write_seq_header(self, count: int) -> None:
        """Write a sequence header (element count as u32)."""
        self.write_u32(count)

    def write_bytes(self, data: bytes | bytearray) -> None:
        """Write raw bytes without length prefix."""
        self._buf.extend(data)

    def write_enum(self, discriminant: int) -> None:
        """Write an enum discriminant value (i32)."""
        self.write_i32(discriminant)

    # -------------------------------------------------------------------------
    # XCDR2 DHEADER (Delimited Header for Appendable/Mutable types)
    # -------------------------------------------------------------------------

    @contextmanager
    def dheader(self) -> Iterator[None]:
        """
        Context manager for writing a DHEADER block.

        Used for appendable and mutable types. Writes a size placeholder,
        yields for content writing, then backpatches the size.

        Example:
            >>> with writer.dheader():
            ...     writer.write_u32(value1)
            ...     writer.write_string(value2)
        """
        self._align(4)
        token = len(self._buf)
        # Write placeholder for object size
        self._buf.extend(b"\x00\x00\x00\x00")
        yield
        # Backpatch the size
        object_size = len(self._buf) - token - 4
        fmt = "<I" if self._le else ">I"
        struct.pack_into(fmt, self._buf, token, object_size)

    def write_dheader_begin(self) -> int:
        """
        Begin a DHEADER block and return a token for finalization.

        Returns:
            Token to pass to write_dheader_finalize()
        """
        self._align(4)
        token = len(self._buf)
        self._buf.extend(b"\x00\x00\x00\x00")
        return token

    def write_dheader_finalize(self, token: int) -> None:
        """Finalize a DHEADER block by backpatching the size."""
        object_size = len(self._buf) - token - 4
        fmt = "<I" if self._le else ">I"
        struct.pack_into(fmt, self._buf, token, object_size)

    # -------------------------------------------------------------------------
    # XCDR2 EMHEADER (Element Member Header for Mutable types)
    # -------------------------------------------------------------------------

    def write_emheader(
        self, member_id: int, data_length: int, must_understand: bool = False
    ) -> None:
        """
        Write an EMHEADER for a mutable type field using LC=4 (NEXTINT) encoding.

        Args:
            member_id: Field member ID (0..0x0FFFFFFF, 28 bits)
            data_length: Length of the field data
            must_understand: Whether the field must be understood
        """
        if member_id > 0x0FFFFFFF:
            raise ValueError(f"EMHEADER member_id exceeds 28 bits: 0x{member_id:X}")
        mu_bit = 0x80000000 if must_understand else 0
        header = mu_bit | (4 << 28) | (member_id & 0x0FFFFFFF)
        self.write_u32(header)
        self.write_u32(data_length)

    @contextmanager
    def emheader(self, member_id: int, must_understand: bool = False) -> Iterator[None]:
        """
        Context manager for writing an EMHEADER block.

        Example:
            >>> with writer.emheader(member_id=0):
            ...     writer.write_u32(field_value)
        """
        token = self.write_emheader_begin(member_id, must_understand)
        yield
        self.write_emheader_finalize(token)

    def write_emheader_begin(self, member_id: int, must_understand: bool = False) -> int:
        """
        Begin an EMHEADER block and return a token for finalization.

        Always uses LC=4 format (8 bytes) for easy backpatching.

        Returns:
            Token to pass to write_emheader_finalize()
        """
        if member_id > 0x0FFFFFFF:
            raise ValueError(f"EMHEADER member_id exceeds 28 bits: 0x{member_id:X}")
        mu_bit = 0x80000000 if must_understand else 0
        header = mu_bit | (4 << 28) | (member_id & 0x0FFFFFFF)
        self.write_u32(header)
        token = len(self._buf)
        self._buf.extend(b"\x00\x00\x00\x00")  # Placeholder for length
        return token

    def write_emheader_finalize(self, token: int) -> None:
        """Finalize an EMHEADER block by backpatching the length."""
        data_length = len(self._buf) - token - 4
        fmt = "<I" if self._le else ">I"
        struct.pack_into(fmt, self._buf, token, data_length)

    def write_sentinel(self) -> None:
        """Write a sentinel marker (end of mutable struct fields)."""
        self.write_u32(MEMBER_ID_SENTINEL)

    # -------------------------------------------------------------------------
    # Output
    # -------------------------------------------------------------------------

    def to_bytes(self) -> bytes:
        """Return the serialized data as bytes."""
        return bytes(self._buf)

    def __len__(self) -> int:
        """Return the current size of the serialized data."""
        return len(self._buf)


class CdrKeyWriter:
    """
    Specialized CDR writer for serializing key fields.

    Key serialization uses big-endian, XCDR2, and no encapsulation header.
    """

    __slots__ = ("_buf",)

    def __init__(self) -> None:
        self._buf = bytearray()

    def _align(self, alignment: int) -> None:
        """Align relative to the start of the key body (post-header offset 0).

        The canonical KeyHash CDR (matching the Rust derive `serialize_key`) is
        big-endian CDR whose alignment is relative to the first byte after the
        stripped encapsulation header, so 8-byte fields (i64/u64/f64) align to 8
        from the buffer start. No cap: XCDR2's 4-byte cap does not apply here.
        """
        if alignment <= 1:
            return
        padding = (alignment - (len(self._buf) % alignment)) % alignment
        if padding > 0:
            self._buf.extend(b"\x00" * padding)

    def write_bool(self, val: bool) -> None:
        self._buf.append(1 if val else 0)

    def write_u8(self, val: int) -> None:
        self._buf.append(val & 0xFF)

    def write_i8(self, val: int) -> None:
        self._buf.extend(struct.pack("b", val))

    def write_u16(self, val: int) -> None:
        self._align(2)
        self._buf.extend(struct.pack(">H", val))

    def write_i16(self, val: int) -> None:
        self._align(2)
        self._buf.extend(struct.pack(">h", val))

    def write_u32(self, val: int) -> None:
        self._align(4)
        self._buf.extend(struct.pack(">I", val))

    def write_i32(self, val: int) -> None:
        self._align(4)
        self._buf.extend(struct.pack(">i", val))

    def write_u64(self, val: int) -> None:
        self._align(8)
        self._buf.extend(struct.pack(">Q", val))

    def write_i64(self, val: int) -> None:
        self._align(8)
        self._buf.extend(struct.pack(">q", val))

    def write_f32(self, val: float) -> None:
        self._align(4)
        self._buf.extend(struct.pack(">f", val))

    def write_f64(self, val: float) -> None:
        self._align(8)
        self._buf.extend(struct.pack(">d", val))

    def write_string(self, val: str) -> None:
        encoded = val.encode("utf-8") + b"\x00"
        self.write_u32(len(encoded))
        self._buf.extend(encoded)

    def write_enum(self, discriminant: int) -> None:
        self.write_i32(discriminant)

    def to_bytes(self) -> bytes:
        return bytes(self._buf)

    def __len__(self) -> int:
        return len(self._buf)
