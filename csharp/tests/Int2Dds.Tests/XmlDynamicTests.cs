using System;
using System.IO;
using System.Linq;
using System.Threading;
using Int2Dds.Core;
using Int2Dds.Xtypes;
using Xunit;

namespace Int2Dds.Tests
{
    /// <summary>
    /// XML-defined runtime types over the C# FFI: load a type from XML, then publish/subscribe
    /// it dynamically (no compile-time IDL). Covers the flat setter path, the full DynamicValue
    /// tree (enum, bitmask, bitset, nested struct, sequence-of-struct, primitive sequence, union,
    /// wide string, map) and the other DDS vendor dialects parsing through the registry.
    /// </summary>
    public class XmlDynamicTests
    {
        private const string FlatXml = @"
<types>
  <struct name=""Telemetry"">
    <member name=""id"" type=""uint32"" key=""true""/>
    <member name=""temperature"" type=""float32""/>
    <member name=""active"" type=""boolean""/>
    <member name=""label"" type=""string""/>
    <member name=""count"" type=""int64""/>
  </struct>
</types>";

        private const string ComplexXml = @"
<types>
  <module name=""t"">
    <enum name=""Mode"">
      <enumerator name=""OFF"" value=""0""/>
      <enumerator name=""ON"" value=""1""/>
    </enum>
    <bitmask name=""Caps"" bit_bound=""8"">
      <bit_value name=""A"" position=""0""/>
      <bit_value name=""B"" position=""1""/>
    </bitmask>
    <bitset name=""Health"">
      <bitfield name=""battery"" bit_bound=""7"" type=""uint8""/>
      <bitfield name=""flags"" bit_bound=""2"" type=""uint8""/>
    </bitset>
    <struct name=""Vec2"">
      <member name=""x"" type=""float32""/>
      <member name=""y"" type=""float32""/>
    </struct>
    <union name=""Cmd"">
      <discriminator type=""int32""/>
      <case><caseDiscriminator value=""0""/><member name=""speed"" type=""float32""/></case>
      <case><caseDiscriminator value=""1""/><member name=""stop"" type=""boolean""/></case>
    </union>
    <struct name=""All"" extensibility=""mutable"">
      <member name=""id"" type=""uint32"" key=""true""/>
      <member name=""mode"" type=""nonBasic"" nonBasicTypeName=""t::Mode""/>
      <member name=""caps"" type=""nonBasic"" nonBasicTypeName=""t::Caps""/>
      <member name=""health"" type=""nonBasic"" nonBasicTypeName=""t::Health""/>
      <member name=""home"" type=""nonBasic"" nonBasicTypeName=""t::Vec2""/>
      <member name=""points"" type=""nonBasic"" nonBasicTypeName=""t::Vec2"" sequenceMaxLength=""-1""/>
      <member name=""samples"" type=""int32"" sequenceMaxLength=""-1""/>
      <member name=""cmd"" type=""nonBasic"" nonBasicTypeName=""t::Cmd""/>
      <member name=""label"" type=""wstring""/>
      <member name=""scores"" type=""int32"" key_type=""string"" mapMaxLength=""8""/>
    </struct>
  </module>
</types>";

        private static void AwaitMatch(DynamicTypeWriter writer, DynamicTypeReader reader, int attempts = 200)
        {
            for (int i = 0; i < attempts; i++)
            {
                if (writer.PublicationMatchedCount() > 0 && reader.SubscriptionMatchedCount() > 0)
                    return;
                Thread.Sleep(50);
            }
            Assert.Fail("writer/reader never matched");
        }

        private static DynamicData Take(DynamicTypeReader reader, int attempts = 200)
        {
            for (int i = 0; i < attempts; i++)
            {
                var sample = reader.Take();
                if (sample != null)
                    return sample;
                Thread.Sleep(50);
            }
            Assert.Fail("no sample received");
            return null!;
        }

        private static string XtypesDir() => AppContext.BaseDirectory;

        [Fact]
        public void XmlFlatRoundTrip()
        {
            using var reg = new XmlTypeRegistry().LoadString(FlatXml);
            Assert.Equal(new[] { "Telemetry" }, reg.TypeNames().ToArray());
            using var support = reg.GetTypeSupport("Telemetry");

            using var dp = new DomainParticipant(71);
            using var topic = dp.CreateTopicDynamic("TelemetryTopic", support);
            using var pub = dp.CreatePublisher();
            using var sub = dp.CreateSubscriber();
            using var writer = pub.CreateDataWriterDynamic(topic, support);
            using var reader = sub.CreateDataReaderDynamic(topic, support);
            AwaitMatch(writer, reader);

            using (var data = support.CreateData())
            {
                data.SetU32("id", 7);
                data.SetF32("temperature", 23.5f);
                data.SetBool("active", true);
                data.SetString("label", "sensor-A");
                data.SetI64("count", -100);
                writer.Write(data);
            }

            using var got = Take(reader);
            Assert.Equal(7u, got.GetU32("id"));
            Assert.Equal(23.5f, got.GetF32("temperature"), 3);
            Assert.True(got.GetBool("active"));
            Assert.Equal("sensor-A", got.GetString("label"));
            Assert.Equal(-100, got.GetI64("count"));
        }

        [Fact]
        public void XmlComplexValueApi()
        {
            using var reg = new XmlTypeRegistry().LoadString(ComplexXml);
            using var support = reg.GetTypeSupport("t::All");
            using var vec2Support = reg.GetTypeSupport("t::Vec2");

            DynamicValue Vec2(float x, float y)
            {
                var d = vec2Support.CreateData();
                d.SetF32("x", x);
                d.SetF32("y", y);
                var v = DynamicValue.Struct(d);
                d.Dispose();
                return v;
            }

            using var dp = new DomainParticipant(72);
            using var topic = dp.CreateTopicDynamic("AllTopic", support);
            using var pub = dp.CreatePublisher();
            using var sub = dp.CreateSubscriber();
            using var writer = pub.CreateDataWriterDynamic(topic, support);
            using var reader = sub.CreateDataReaderDynamic(topic, support);
            AwaitMatch(writer, reader);

            using (var data = support.CreateData())
            {
                data.SetU32("id", 1);
                data.SetValue("mode", DynamicValue.Enum("ON", 1));
                data.SetValue("caps", DynamicValue.Bitmask(0b11));
                data.SetValue("health", DynamicValue.Bitset((2UL << 7) | 80));

                data.SetValue("home", Vec2(1.5f, -2.5f));

                var points = DynamicValue.Sequence();
                points.Push(Vec2(10.0f, 11.0f)).Push(Vec2(20.0f, 21.0f));
                data.SetValue("points", points);

                var samples = DynamicValue.Sequence();
                samples.Push(DynamicValue.I32(100)).Push(DynamicValue.I32(200));
                data.SetValue("samples", samples);

                data.SetValue("cmd", DynamicValue.Union(DynamicValue.I32(1), DynamicValue.Bool(true)));

                data.SetValue("label", DynamicValue.WString("로봇"));

                var scores = DynamicValue.Map();
                scores.MapInsert(DynamicValue.String("alpha"), DynamicValue.I32(90));
                scores.MapInsert(DynamicValue.String("beta"), DynamicValue.I32(80));
                data.SetValue("scores", scores);

                writer.Write(data);
            }

            using var got = Take(reader);
            Assert.Equal(1u, got.GetU32("id"));

            using (var mode = got.GetValue("mode"))
            {
                Assert.Equal(DynamicValueKind.Enum, mode.Kind());
                Assert.Equal(("ON", 1), mode.AsEnum());
            }

            using (var caps = got.GetValue("caps"))
                Assert.Equal(0b11UL, caps.AsBitmask());
            using (var health = got.GetValue("health"))
                Assert.Equal((2UL << 7) | 80, health.AsBitset());

            Assert.Equal(1.5f, got.GetF32("home.x"), 3);
            Assert.Equal(-2.5f, got.GetF32("home.y"), 3);

            using (var pts = got.GetValue("points"))
            {
                Assert.Equal(DynamicValueKind.Sequence, pts.Kind());
                Assert.Equal(2, pts.Length());
            }
            Assert.Equal(20.0f, got.GetF32("points[1].x"), 3);

            Assert.Equal(100, got.GetI32("samples[0]"));
            Assert.Equal(200, got.GetI32("samples[1]"));

            using (var cmd = got.GetValue("cmd"))
            using (var disc = cmd.UnionDiscriminator())
            using (var branch = cmd.UnionValue())
            {
                Assert.Equal(1, disc.AsI32());
                Assert.True(branch.AsBool());
            }

            using (var label = got.GetValue("label"))
                Assert.Equal("로봇", label.AsString());

            using (var scoresV = got.GetValue("scores"))
            {
                var readback = Enumerable.Range(0, scoresV.Length()).ToDictionary(
                    i => { using var k = scoresV.MapKey(i); return k.AsString(); },
                    i => { using var v = scoresV.MapValue(i); return v.AsI32(); });
                Assert.Equal(90, readback["alpha"]);
                Assert.Equal(80, readback["beta"]);
            }
        }

        [Theory]
        [InlineData("sensor_data.xml")]
        public void SampleXmlParses(string filename)
        {
            var path = Path.Combine(XtypesDir(), filename);
            if (!File.Exists(path))
                return; // sample not present

            using var reg = XmlTypeRegistry.FromFile(path);
            var names = reg.TypeNames();
            Assert.NotEmpty(names);
            foreach (var name in names)
            {
                using var support = reg.GetTypeSupport(name);
                using var obj = reg.GetTypeObject(name);
            }
        }
    }
}
