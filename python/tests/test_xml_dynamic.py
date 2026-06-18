"""
XML-defined runtime types over the Python FFI: load a type from XML, then
publish/subscribe it dynamically (no compile-time IDL). Covers the flat setter
path, the full DynamicValue tree (enum, bitmask, bitset, nested struct,
sequence-of-struct, primitive sequence, union, wide string, map) and the
other DDS vendor dialects parsing through the registry.
"""

from __future__ import annotations

import time
from pathlib import Path

import pytest

from int2dds import DomainParticipant
from int2dds.types import DynamicValue, XmlTypeRegistry
from int2dds.types.dynamic import (
    VALUE_KIND_ENUM,
    VALUE_KIND_SEQUENCE,
)

XTYPES_DIR = Path(__file__).resolve().parents[2] / "dds" / "examples" / "xtypes"

FLAT_XML = """
<types>
  <struct name="Telemetry">
    <member name="id" type="uint32" key="true"/>
    <member name="temperature" type="float32"/>
    <member name="active" type="boolean"/>
    <member name="label" type="string"/>
    <member name="count" type="int64"/>
  </struct>
</types>
"""

COMPLEX_XML = """
<types>
  <module name="t">
    <enum name="Mode">
      <enumerator name="OFF" value="0"/>
      <enumerator name="ON" value="1"/>
    </enum>
    <bitmask name="Caps" bit_bound="8">
      <bit_value name="A" position="0"/>
      <bit_value name="B" position="1"/>
    </bitmask>
    <bitset name="Health">
      <bitfield name="battery" bit_bound="7" type="uint8"/>
      <bitfield name="flags" bit_bound="2" type="uint8"/>
    </bitset>
    <struct name="Vec2">
      <member name="x" type="float32"/>
      <member name="y" type="float32"/>
    </struct>
    <union name="Cmd">
      <discriminator type="int32"/>
      <case><caseDiscriminator value="0"/><member name="speed" type="float32"/></case>
      <case><caseDiscriminator value="1"/><member name="stop" type="boolean"/></case>
    </union>
    <struct name="All" extensibility="mutable">
      <member name="id" type="uint32" key="true"/>
      <member name="mode" type="nonBasic" nonBasicTypeName="t::Mode"/>
      <member name="caps" type="nonBasic" nonBasicTypeName="t::Caps"/>
      <member name="health" type="nonBasic" nonBasicTypeName="t::Health"/>
      <member name="home" type="nonBasic" nonBasicTypeName="t::Vec2"/>
      <member name="points" type="nonBasic" nonBasicTypeName="t::Vec2" sequenceMaxLength="-1"/>
      <member name="samples" type="int32" sequenceMaxLength="-1"/>
      <member name="cmd" type="nonBasic" nonBasicTypeName="t::Cmd"/>
      <member name="label" type="wstring"/>
      <member name="scores" type="int32" key_type="string" mapMaxLength="8"/>
    </struct>
  </module>
</types>
"""


def _await_match(writer, reader, attempts: int = 200) -> None:
    for _ in range(attempts):
        if writer.publication_matched_count() > 0 and reader.subscription_matched_count() > 0:
            return
        time.sleep(0.05)
    pytest.fail("writer/reader never matched")


def _take(reader, attempts: int = 200):
    for _ in range(attempts):
        sample = reader.take()
        if sample is not None:
            return sample
        time.sleep(0.05)
    pytest.fail("no sample received")


def test_xml_flat_round_trip():
    reg = XmlTypeRegistry().load_str(FLAT_XML)
    assert reg.type_names() == ["Telemetry"]
    support = reg.get_type_support("Telemetry")

    with DomainParticipant(domain_id=61) as dp:
        topic = dp.create_topic_dynamic("TelemetryTopic", support)
        pub = dp.create_publisher()
        sub = dp.create_subscriber()
        writer = pub.create_datawriter_dynamic(topic, support)
        reader = sub.create_datareader_dynamic(topic, support)
        _await_match(writer, reader)

        data = support.create_data()
        data.set_u32("id", 7)
        data.set_f32("temperature", 23.5)
        data.set_bool("active", True)
        data.set_string("label", "sensor-A")
        data.set_i64("count", -100)
        writer.write(data)

        got = _take(reader)
        assert got.get_u32("id") == 7
        assert got.get_f32("temperature") == pytest.approx(23.5)
        assert got.get_bool("active") is True
        assert got.get_string("label") == "sensor-A"
        assert got.get_i64("count") == -100


def test_xml_complex_value_api():
    reg = XmlTypeRegistry().load_str(COMPLEX_XML)
    support = reg.get_type_support("t::All")
    vec2_support = reg.get_type_support("t::Vec2")

    def vec2(x: float, y: float) -> DynamicValue:
        d = vec2_support.create_data()
        d.set_f32("x", x)
        d.set_f32("y", y)
        return DynamicValue.struct(d)

    with DomainParticipant(domain_id=62) as dp:
        topic = dp.create_topic_dynamic("AllTopic", support)
        pub = dp.create_publisher()
        sub = dp.create_subscriber()
        writer = pub.create_datawriter_dynamic(topic, support)
        reader = sub.create_datareader_dynamic(topic, support)
        _await_match(writer, reader)

        data = support.create_data()
        data.set_u32("id", 1)
        data.set_value("mode", DynamicValue.enum("ON", 1))
        data.set_value("caps", DynamicValue.bitmask(0b11))
        data.set_value("health", DynamicValue.bitset((2 << 7) | 80))

        data.set_value("home", vec2(1.5, -2.5))

        points = DynamicValue.sequence()
        for x, y in ((10.0, 11.0), (20.0, 21.0)):
            points.push(vec2(x, y))
        data.set_value("points", points)

        samples = DynamicValue.sequence()
        samples.push(DynamicValue.i32(100)).push(DynamicValue.i32(200))
        data.set_value("samples", samples)

        cmd = DynamicValue.union(DynamicValue.i32(1), DynamicValue.boolean(True))
        data.set_value("cmd", cmd)

        data.set_value("label", DynamicValue.wstring("로봇"))

        scores = DynamicValue.map()
        scores.map_insert(DynamicValue.string("alpha"), DynamicValue.i32(90))
        scores.map_insert(DynamicValue.string("beta"), DynamicValue.i32(80))
        data.set_value("scores", scores)

        writer.write(data)
        got = _take(reader)

        assert got.get_u32("id") == 1

        mode = got.get_value("mode")
        assert mode.kind() == VALUE_KIND_ENUM
        assert mode.as_enum() == ("ON", 1)

        assert got.get_value("caps").as_bitmask() == 0b11
        assert got.get_value("health").as_bitset() == (2 << 7) | 80

        assert got.get_f32("home.x") == pytest.approx(1.5)
        assert got.get_f32("home.y") == pytest.approx(-2.5)

        pts = got.get_value("points")
        assert pts.kind() == VALUE_KIND_SEQUENCE
        assert pts.len() == 2
        assert got.get_f32("points[1].x") == pytest.approx(20.0)

        assert got.get_i32("samples[0]") == 100
        assert got.get_i32("samples[1]") == 200

        cmd_v = got.get_value("cmd")
        assert cmd_v.union_discriminator().as_i32() == 1
        assert cmd_v.union_value().as_bool() is True

        assert got.get_value("label").as_string() == "로봇"

        scores_v = got.get_value("scores")
        readback = {
            scores_v.map_key(i).as_string(): scores_v.map_value(i).as_i32()
            for i in range(scores_v.len())
        }
        assert readback == {"alpha": 90, "beta": 80}


@pytest.mark.parametrize("filename", ["sensor_data.xml"])
def test_sample_xml_parses(filename):
    path = XTYPES_DIR / filename
    if not path.exists():
        pytest.skip(f"sample XML not present: {path}")

    reg = XmlTypeRegistry.from_file(str(path))
    names = reg.type_names()
    assert names, f"{filename} declared no types"
    for name in names:
        support = reg.get_type_support(name)
        assert support.handle is not None
        obj = reg.get_type_object(name)
        assert obj.handle is not None
        obj.close()
        support.close()
