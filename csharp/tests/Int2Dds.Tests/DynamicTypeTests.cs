using System.Linq;
using Int2Dds.Cdr;
using Int2Dds.Core;
using Int2Dds.TypeInfo;
using Int2Dds.Types;
using Int2Dds.Xtypes;
using Xunit;

namespace Int2Dds.Tests
{
    public class DynamicTypeTests
    {
        [Fact]
        public void TypeInfoBuilder_BuildsAndIntrospectsComposite()
        {
            using (var b = new TypeInfoBuilder("Composite", Extensibility.Appendable))
            {
                b.AddField("id", FieldType.Int32, true)
                 .AddNamedTypeField("leaf", "Leaf")
                 .AddSequenceOfNamedField("leaves", "Leaf")
                 .AddArrayOfNamedField("fixed", "Leaf", 3);

                using (var obj = b.ToTypeObject())
                {
                    Assert.Equal((int)Extensibility.Appendable, obj.Extensibility);
                    Assert.Equal(4u, obj.MemberCount);

                    var members = obj.Members();
                    Assert.Equal(new[] { "id", "leaf", "leaves", "fixed" }, members.Select(m => m.Name).ToArray());
                    Assert.Equal(FieldType.Nested, members[1].Kind);
                    Assert.Equal(FieldType.Sequence, members[2].Kind);
                    Assert.Equal(FieldType.Array, members[3].Kind);
                    Assert.Equal(2u, obj.FindMember("leaves"));
                }
            }
        }

        [Fact]
        public void DynamicData_DecodesFlatSample()
        {
            using (var dp = new DomainParticipant(0))
            using (var b = new TypeInfoBuilder("FlatType", Extensibility.Appendable))
            {
                b.AddField("id", FieldType.Int32, true)
                 .AddField("value", FieldType.Float64)
                 .AddField("label", FieldType.String)
                 .AddField("flag", FieldType.Bool);

                using (var typeObj = b.ToTypeObject())
                {
                    var w = new CdrWriter(Extensibility.Appendable, true, true);
                    int token = w.DheaderBegin();
                    w.WriteI32(7);
                    w.WriteF64(3.5);
                    w.WriteString("hi");
                    w.WriteBool(true);
                    w.DheaderFinalize(token);
                    byte[] data = w.ToBytes();

                    using (var dd = dp.DecodeSample(typeObj, data))
                    {
                        Assert.Equal(7, dd.GetI32("id"));
                        Assert.Equal(3.5, dd.GetF64("value"));
                        Assert.Equal("hi", dd.GetString("label"));
                        Assert.True(dd.GetBool("flag"));
                    }
                }
            }
        }
    }
}
