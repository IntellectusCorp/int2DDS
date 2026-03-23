"""
Example HelloWorld data type.

This is what int2dds-idl would generate from:

    struct HelloWorld {
        unsigned long index;
        string message;
    };
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import ClassVar

from int2dds.cdr import CdrReader, CdrWriter, Extensibility
from int2dds.cdr.writer import CdrKeyWriter


@dataclass
class HelloWorld:
    """IDL struct: HelloWorld (Extensibility: FINAL)"""

    _dds_type_name: ClassVar[str] = "HelloWorld"
    _extensibility: ClassVar[Extensibility] = Extensibility.FINAL
    _has_key: ClassVar[bool] = False

    index: int = 0
    message: str = ""

    def _serialize_cdr(self) -> bytes:
        """Serialize to CDR bytes with encapsulation header."""
        w = CdrWriter(extensibility=self._extensibility)
        w.write_u32(self.index)
        w.write_string(self.message)
        return w.to_bytes()

    @classmethod
    def _deserialize_cdr(cls, data: bytes) -> "HelloWorld":
        """Deserialize from CDR bytes."""
        r = CdrReader(data)
        index = r.read_u32()
        message = r.read_string()
        return cls(index, message)

    def _serialize_key(self) -> bytes:
        """Serialize key fields only (big-endian, no encapsulation)."""
        return b""
