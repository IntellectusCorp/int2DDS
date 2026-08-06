using Int2Dds.Cdr;
using Int2Dds.Core;
using Int2Dds.Types;
using Xunit;

namespace Int2Dds.Tests
{
    /// <summary>
    /// Flat keyed type carrying generator-style advertisement metadata (a generated ShapeType).
    /// Creating a topic for it must go through int2dds_create_topic_with_type_info and advertise
    /// a conformant TypeObject, matching the Rust derive.
    /// </summary>
    [DdsType("AdShape", 1, true)] // Appendable, keyed
    public class AdShape : IDdsType
    {
        // (op, name, typeConst, size, flags): INT2DDS_FIELD_* constants, MEMBER_KEY=1.
        public static readonly DdsTypeInfoField[] DdsTypeInfoFields = new DdsTypeInfoField[]
        {
            new DdsTypeInfoField("string", "color", 0, 128u, 1),
            new DdsTypeInfoField("field", "x", 5, 0u, 0),
            new DdsTypeInfoField("field", "shapesize", 5, 0u, 0),
            new DdsTypeInfoField("seq", "payload", 7, 0u, 0),
        };

        public string Color { get; set; } = "BLUE";
        public int X { get; set; }
        public int Shapesize { get; set; } = 30;
        public byte[] Payload { get; set; } = new byte[0];

        public AdShape() { }

        public byte[] SerializeCdr() => SerializeCdr(false);

        public byte[] SerializeCdr(bool xcdr2)
        {
            var w = new CdrWriter(Extensibility.Appendable, true, xcdr2);
            SerializeCdr(w);
            return w.ToBytes();
        }

        public void SerializeCdr(CdrWriter w)
        {
            int token = w.DheaderBegin();
            w.WriteString(Color);
            w.WriteI32(X);
            w.WriteI32(Shapesize);
            w.WriteSeqHeader((uint)Payload.Length);
            foreach (var b in Payload)
                w.WriteU8(b);
            w.DheaderFinalize(token);
        }
    }

    public class TypeInfoAdvertiseTests
    {
        [Fact]
        public void AdvertisedTopicAndEndpointsCreate()
        {
            using (var dp = new DomainParticipant(0))
            using (var topic = dp.CreateTopic<AdShape>("AdShapeTopic"))
            {
                // Topic creation succeeding means BuildTypeInfo + create_topic_with_type_info worked.
                Assert.Equal("AdShape", topic.TypeName);

                var pub = dp.CreatePublisher();
                var writer = pub.CreateDataWriter(topic);
                var sub = dp.CreateSubscriber();
                var reader = sub.CreateDataReader(topic);
                Assert.NotNull(writer);
                Assert.NotNull(reader);
            }
        }

        [Fact]
        public void AdvertisedKeyedInstanceHandleIsNonNil()
        {
            using (var dp = new DomainParticipant(0))
            using (var topic = dp.CreateTopic<AdShape>("AdShapeKeyed"))
            {
                var pub = dp.CreatePublisher();
                var writer = pub.CreateDataWriter(topic);
                var handle = writer.RegisterInstance(new AdShape { Color = "GREEN", X = 1 });
                Assert.False(handle.IsNil);
            }
        }
    }
}
