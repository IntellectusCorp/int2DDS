"""Drift guards pinning Python wire defaults: Appendable extensibility, XCDR1."""

from int2dds.cdr import CdrWriter, Extensibility
from int2dds.core.qos import DataRepresentation


def test_default_data_representation_is_xcdr1():
    assert DataRepresentation().kind == "XCDR1"


def test_default_cdr_writer_extensibility_is_appendable():
    assert CdrWriter()._extensibility == Extensibility.APPENDABLE
