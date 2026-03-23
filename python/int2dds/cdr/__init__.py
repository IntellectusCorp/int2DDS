"""
CDR/XCDR2 Serialization Library for int2dds.

Provides pure Python CDR (Common Data Representation) serialization
for DDS data types.
"""

from int2dds.cdr.reader import CdrReader
from int2dds.cdr.writer import CdrWriter, Extensibility

__all__ = [
    "CdrWriter",
    "CdrReader",
    "Extensibility",
]
