"""
CDR/XCDR2 Reader for deserializing DDS data types.
"""

from __future__ import annotations

import struct
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    pass

# Encapsulation IDs
_ENCAP_CDR_BE = 0x0000
_ENCAP_CDR_LE = 0x0001
_ENCAP_PL_CDR_BE = 0x0002
_ENCAP_PL_CDR_LE = 0x0003
_ENCAP_CDR2_BE = 0x0006
_ENCAP_CDR2_LE = 0x0007
_ENCAP_DCDR2_BE = 0x0008
_ENCAP_DCDR2_LE = 0x0009
_ENCAP_PL_CDR2_BE = 0x000A
_ENCAP_PL_CDR2_LE = 0x000B

# Sentinel for mutable types
MEMBER_ID_SENTINEL = 0x3F02


class CdrError(Exception):
    """Base class for CDR errors."""

    pass


class CdrUnderflowError(CdrError):
    """Not enough data remaining in buffer."""

    pass


class CdrInvalidEncapsulationError(CdrError):
    """Unrecognized encapsulation ID."""

    pass


class CdrReader:
    """
    CDR/XCDR2 deserialization reader.

    Reads data in CDR (Common Data Representation) format with automatic
    detection of XCDR2 extensions.

    Example:
        >>> reader = CdrReader(data)
        >>> index = reader.read_u32()
        >>> message = reader.read_string()
    """

    __slots__ = ("_buf", "_pos", "_le", "_xcdr2", "_header_size")

    def __init__(self, data: bytes | bytearray | memoryview) -> None:
        """
        Initialize a CDR reader from serialized data.

        Args:
            data: The CDR-encoded bytes to read

        Raises:
            CdrUnderflowError: If data is too small for encapsulation header
            CdrInvalidEncapsulationError: If encapsulation ID is unrecognized
        """
        self._buf = memoryview(data) if not isinstance(data, memoryview) else data

        if len(self._buf) < 4:
            raise CdrUnderflowError("Buffer too small for encapsulation header")

        # Parse encapsulation header (always big-endian)
        encap_id = struct.unpack(">H", self._buf[0:2])[0]

        # Determine endianness and XCDR version
        if encap_id in (_ENCAP_CDR_LE, _ENCAP_PL_CDR_LE):
            self._le = True
            self._xcdr2 = False
        elif encap_id in (_ENCAP_CDR_BE, _ENCAP_PL_CDR_BE):
            self._le = False
            self._xcdr2 = False
        elif encap_id in (_ENCAP_CDR2_LE, _ENCAP_DCDR2_LE, _ENCAP_PL_CDR2_LE):
            self._le = True
            self._xcdr2 = True
        elif encap_id in (_ENCAP_CDR2_BE, _ENCAP_DCDR2_BE, _ENCAP_PL_CDR2_BE):
            self._le = False
            self._xcdr2 = True
        else:
            raise CdrInvalidEncapsulationError(f"Unknown encapsulation ID: 0x{encap_id:04X}")

        self._header_size = 4
        self._pos = 4  # Skip encapsulation header

    @classmethod
    def from_raw(
        cls,
        data: bytes | bytearray | memoryview,
        little_endian: bool = True,
        xcdr2: bool = False,
    ) -> CdrReader:
        """
        Create a reader without encapsulation header.

        Args:
            data: Raw CDR data (no encapsulation header)
            little_endian: Byte order of the data
            xcdr2: Whether data uses XCDR2 encoding

        Returns:
            CdrReader instance
        """
        reader = object.__new__(cls)
        reader._buf = memoryview(data) if not isinstance(data, memoryview) else data
        reader._pos = 0
        reader._header_size = 0
        reader._le = little_endian
        reader._xcdr2 = xcdr2
        return reader

    def _ensure(self, n: int) -> None:
        """Ensure at least n bytes are available."""
        if self._pos + n > len(self._buf):
            raise CdrUnderflowError(
                f"Need {n} bytes but only {len(self._buf) - self._pos} remaining"
            )

    def _align(self, alignment: int) -> None:
        """Align the read position to the given boundary."""
        if alignment <= 1:
            return

        # XCDR2: max alignment capped at 4 bytes
        actual = min(alignment, 4) if self._xcdr2 else alignment

        # Alignment is relative to data start (after encapsulation header)
        stream_pos = self._pos - self._header_size
        padding = (actual - (stream_pos % actual)) % actual
        new_pos = self._pos + padding

        if new_pos > len(self._buf):
            raise CdrUnderflowError(f"Alignment would exceed buffer")

        self._pos = new_pos

    @property
    def remaining(self) -> int:
        """Return the number of bytes remaining."""
        return max(0, len(self._buf) - self._pos)

    @property
    def position(self) -> int:
        """Return the current read position."""
        return self._pos

    # -------------------------------------------------------------------------
    # Primitive reads
    # -------------------------------------------------------------------------

    def read_bool(self) -> bool:
        """Read a boolean value (1 byte)."""
        self._ensure(1)
        val = self._buf[self._pos] != 0
        self._pos += 1
        return val

    def read_u8(self) -> int:
        """Read an unsigned 8-bit integer."""
        self._ensure(1)
        val = self._buf[self._pos]
        self._pos += 1
        return val

    def read_i8(self) -> int:
        """Read a signed 8-bit integer."""
        self._ensure(1)
        val = struct.unpack_from("b", self._buf, self._pos)[0]
        self._pos += 1
        return val

    def read_u16(self) -> int:
        """Read an unsigned 16-bit integer."""
        self._align(2)
        self._ensure(2)
        fmt = "<H" if self._le else ">H"
        val = struct.unpack_from(fmt, self._buf, self._pos)[0]
        self._pos += 2
        return val

    def read_i16(self) -> int:
        """Read a signed 16-bit integer."""
        self._align(2)
        self._ensure(2)
        fmt = "<h" if self._le else ">h"
        val = struct.unpack_from(fmt, self._buf, self._pos)[0]
        self._pos += 2
        return val

    def read_u32(self) -> int:
        """Read an unsigned 32-bit integer."""
        self._align(4)
        self._ensure(4)
        fmt = "<I" if self._le else ">I"
        val = struct.unpack_from(fmt, self._buf, self._pos)[0]
        self._pos += 4
        return val

    def read_i32(self) -> int:
        """Read a signed 32-bit integer."""
        self._align(4)
        self._ensure(4)
        fmt = "<i" if self._le else ">i"
        val = struct.unpack_from(fmt, self._buf, self._pos)[0]
        self._pos += 4
        return val

    def read_u64(self) -> int:
        """Read an unsigned 64-bit integer."""
        self._align(4 if self._xcdr2 else 8)
        self._ensure(8)
        fmt = "<Q" if self._le else ">Q"
        val = struct.unpack_from(fmt, self._buf, self._pos)[0]
        self._pos += 8
        return val

    def read_i64(self) -> int:
        """Read a signed 64-bit integer."""
        self._align(4 if self._xcdr2 else 8)
        self._ensure(8)
        fmt = "<q" if self._le else ">q"
        val = struct.unpack_from(fmt, self._buf, self._pos)[0]
        self._pos += 8
        return val

    def read_f32(self) -> float:
        """Read a 32-bit float."""
        self._align(4)
        self._ensure(4)
        fmt = "<f" if self._le else ">f"
        val = struct.unpack_from(fmt, self._buf, self._pos)[0]
        self._pos += 4
        return val

    def read_f64(self) -> float:
        """Read a 64-bit double."""
        self._align(4 if self._xcdr2 else 8)
        self._ensure(8)
        fmt = "<d" if self._le else ">d"
        val = struct.unpack_from(fmt, self._buf, self._pos)[0]
        self._pos += 8
        return val

    def read_char(self) -> str:
        """Read a single character (1 byte)."""
        self._ensure(1)
        val = chr(self._buf[self._pos])
        self._pos += 1
        return val

    # -------------------------------------------------------------------------
    # String and sequence
    # -------------------------------------------------------------------------

    def read_string(self) -> str:
        """
        Read a length-prefixed string.

        Returns:
            The decoded UTF-8 string (without null terminator)
        """
        length = self.read_u32()
        if length == 0:
            return ""

        self._ensure(length)
        # Exclude null terminator
        data = bytes(self._buf[self._pos : self._pos + length - 1])
        self._pos += length
        return data.decode("utf-8")

    def read_wstring(self) -> str:
        """
        Read a length-prefixed wide string (UTF-16).

        Length is the number of UTF-16 code units (NOT bytes, NOT including null).
        No null terminator is read.
        """
        code_units = self.read_u32()
        if code_units == 0:
            return ""

        self._align(2)
        byte_count = code_units * 2
        self._ensure(byte_count)
        data = bytes(self._buf[self._pos : self._pos + byte_count])
        self._pos += byte_count
        encoding = "utf-16-le" if self._le else "utf-16-be"
        return data.decode(encoding)

    def read_seq_header(self) -> int:
        """Read a sequence header (element count)."""
        count = self.read_u32()
        
        if count > self.remaining:
            raise CdrUnderflowError(
                f"Sequence length {count} exceeds {self.remaining} remaining bytes"
            )
        return count

    def read_bytes(self, length: int) -> bytes:
        """Read raw bytes."""
        self._ensure(length)
        data = bytes(self._buf[self._pos : self._pos + length])
        self._pos += length
        return data

    def read_enum(self) -> int:
        """Read an enum discriminant value (i32)."""
        return self.read_i32()

    def skip(self, n: int) -> None:
        """Skip n bytes."""
        self._ensure(n)
        self._pos += n

    # -------------------------------------------------------------------------
    # XCDR2 DHEADER (Delimited Header)
    # -------------------------------------------------------------------------

    def read_dheader(self) -> tuple[int, int]:
        """
        Read a DHEADER and return (object_size, start_position).

        Returns:
            Tuple of (object_size, start_position) for use with read_dheader_end()
        """
        object_size = self.read_u32()
        start_pos = self._pos
        return object_size, start_pos

    def read_dheader_end(self, object_size: int, start_pos: int) -> None:
        """
        End a DHEADER block by skipping any remaining bytes.

        Args:
            object_size: The object size from read_dheader()
            start_pos: The start position from read_dheader()
        """
        expected_end = start_pos + object_size
        if expected_end > len(self._buf):
            raise CdrUnderflowError("DHEADER object extends beyond buffer")
        self._pos = expected_end

    # -------------------------------------------------------------------------
    # XCDR2 EMHEADER (Element Member Header)
    # -------------------------------------------------------------------------

    def read_emheader(self) -> tuple[int, int, bool]:
        """
        Read an EMHEADER for a mutable type field.

        Returns:
            Tuple of (member_id, data_length, must_understand)
        """
        header = self.read_u32()

        must_understand = bool(header & 0x80000000)
        lc = (header >> 28) & 0x07
        member_id = header & 0x0FFFFFFF

        if lc == 0:
            data_length = 1
        elif lc == 1:
            data_length = 2
        elif lc == 2:
            data_length = 4
        elif lc == 3:
            data_length = 8
        elif lc == 4:
            data_length = self.read_u32()
        else:
            if self._pos + 4 > len(self._buf):
                raise CdrUnderflowError("NEXTINT extends beyond buffer")
            fmt = "<I" if self._le else ">I"
            nextint = struct.unpack_from(fmt, self._buf, self._pos)[0]
            if lc == 5:
                data_length = nextint
            elif lc == 6:
                data_length = 4 + 4 * nextint
            else:  # lc == 7
                data_length = 4 + 8 * nextint

        return member_id, data_length, must_understand

    def is_sentinel(self) -> bool:
        """Check if the next bytes are a sentinel marker."""
        if self._pos + 4 > len(self._buf):
            return False

        fmt = "<I" if self._le else ">I"
        header = struct.unpack_from(fmt, self._buf, self._pos)[0]
        member_id = header & 0x0FFFFFFF
        return member_id == MEMBER_ID_SENTINEL

    def skip_sentinel(self) -> None:
        """Skip over a sentinel marker."""
        if self.is_sentinel():
            self._pos += 4
