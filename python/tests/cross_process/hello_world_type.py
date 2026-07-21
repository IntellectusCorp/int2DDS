"""
HelloWorld data type for cross-process communication tests.

Matches the Rust HelloWorldType CDR layout:
    struct HelloWorld {
        unsigned long index;   // u32
        string message;
    };
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import ClassVar

from int2dds.cdr import CdrReader, CdrWriter, Extensibility


@dataclass
class HelloWorld:
    """IDL struct: HelloWorld (Extensibility: FINAL)"""

    _dds_type_name: ClassVar[str] = "HelloWorld"
    _extensibility: ClassVar[Extensibility] = Extensibility.FINAL
    _has_key: ClassVar[bool] = False

    index: int = 0
    message: str = ""

    def _serialize_cdr(self, xcdr2: bool = False) -> bytes:
        w = CdrWriter(extensibility=self._extensibility, xcdr2=xcdr2)
        w.write_u32(self.index)
        w.write_string(self.message)
        return w.to_bytes()

    @classmethod
    def _deserialize_cdr(cls, data: bytes) -> "HelloWorld":
        r = CdrReader(data)
        return cls(index=r.read_u32(), message=r.read_string())
