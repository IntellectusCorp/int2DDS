"""
ShapeType data type for cross-process interoperability tests.

Matches the OMG DDS interoperability ShapeType:
    @appendable
    struct ShapeType {
        @key string color;
        long x;
        long y;
        long shapesize;
    };
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar, List

from int2dds.cdr import CdrReader, CdrWriter, Extensibility
from int2dds.cdr.writer import CdrKeyWriter


@dataclass
class ShapeType:
    """OMG DDS Interoperability ShapeType (Extensibility: APPENDABLE)"""

    _dds_type_name: ClassVar[str] = "ShapeType"
    _extensibility: ClassVar[Extensibility] = Extensibility.APPENDABLE
    _has_key: ClassVar[bool] = True

    color: str = "BLUE"
    x: int = 0
    y: int = 0
    shapesize: int = 20
    additional_payload_size: bytes = b""

    def _serialize_cdr(self, xcdr2: bool = False) -> bytes:
        w = CdrWriter(extensibility=self._extensibility, xcdr2=xcdr2)
        if xcdr2:
            with w.dheader():
                w.write_string(self.color)
                w.write_i32(self.x)
                w.write_i32(self.y)
                w.write_i32(self.shapesize)
                w.write_seq_header(len(self.additional_payload_size))
                if self.additional_payload_size:
                    w.write_bytes(self.additional_payload_size)
        else:
            w.write_string(self.color)
            w.write_i32(self.x)
            w.write_i32(self.y)
            w.write_i32(self.shapesize)
            w.write_seq_header(len(self.additional_payload_size))
            if self.additional_payload_size:
                w.write_bytes(self.additional_payload_size)
        return w.to_bytes()

    @classmethod
    def _deserialize_cdr(cls, data: bytes) -> "ShapeType":
        r = CdrReader(data)
        # Skip DHEADER if present (XCDR2 Appendable encoding)
        if r._xcdr2:
            r.read_dheader()
        color = r.read_string()
        x = r.read_i32()
        y = r.read_i32()
        shapesize = r.read_i32()
        # Read additional_payload_size if remaining data exists
        additional_payload_size = b""
        if r.remaining > 4:
            seq_len = r.read_u32()
            if seq_len > 0:
                additional_payload_size = r.read_bytes(seq_len)
        return cls(color=color, x=x, y=y, shapesize=shapesize,
                   additional_payload_size=additional_payload_size)

    def _serialize_key(self) -> bytes:
        """Serialize key fields only (color is the key)."""
        w = CdrKeyWriter()
        w.write_string(self.color)
        return w.to_bytes()
