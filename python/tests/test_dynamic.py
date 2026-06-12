"""
Dynamic-type tests: TypeInfo builder, TypeObject introspection and DynamicData
decoding (flat, in-process — nested resolution over discovery is covered by the
Rust two-participant TypeLookup tests).
"""

from __future__ import annotations

import pytest

from int2dds import DomainParticipant
from int2dds.cdr.writer import CdrWriter, Extensibility
from int2dds.types.dynamic import (
    FIELD_BOOL,
    FIELD_FLOAT64,
    FIELD_INT32,
    FIELD_STRING,
    MEMBER_KEY,
    TypeInfoBuilder,
)


def _flat_type_object():
    b = TypeInfoBuilder("FlatType", Extensibility.APPENDABLE)
    b.add_field("id", FIELD_INT32, MEMBER_KEY)
    b.add_field("value", FIELD_FLOAT64)
    b.add_field("label", FIELD_STRING)
    b.add_field("flag", FIELD_BOOL)
    obj = b.to_type_object()
    b.close()
    return obj


def _flat_bytes(id_: int, value: float, label: str, flag: bool) -> bytes:
    w = CdrWriter(extensibility=Extensibility.APPENDABLE, xcdr2=True)
    with w.dheader():
        w.write_i32(id_)
        w.write_f64(value)
        w.write_string(label)
        w.write_bool(flag)
    return w.to_bytes()


def test_typeinfo_builder_and_introspection():
    b = TypeInfoBuilder("Composite", Extensibility.APPENDABLE)
    b.add_field("id", FIELD_INT32, MEMBER_KEY)
    b.add_named_type_field("leaf", "Leaf")
    b.add_sequence_of_named_field("leaves", "Leaf", 0)
    b.add_array_of_named_field("fixed", "Leaf", 3)
    obj = b.to_type_object()
    try:
        assert obj.extensibility == int(Extensibility.APPENDABLE)
        assert obj.member_count() == 4
        names = [m["name"] for m in obj.members()]
        assert names == ["id", "leaf", "leaves", "fixed"]
        assert obj.members()[0]["flags"] & MEMBER_KEY
        assert obj.find_member("leaves") == 2
    finally:
        obj.close()
        b.close()


def test_dynamic_data_flat_decode():
    dp = DomainParticipant(domain_id=0, name="PyDynTest")
    try:
        type_obj = _flat_type_object()
        data = _flat_bytes(7, 3.5, "hi", True)
        dd = dp.decode_sample(type_obj, data)
        try:
            assert dd.get_i32("id") == 7
            assert dd.get_f64("value") == 3.5
            assert dd.get_string("label") == "hi"
            assert dd.get_bool("flag") is True
            # generic kind-dispatched access via TypeObject member kind
            label_kind = type_obj.members()[2]["kind"]
            assert dd.get("label", label_kind) == "hi"
        finally:
            dd.close()
            type_obj.close()
    finally:
        dp.close()
