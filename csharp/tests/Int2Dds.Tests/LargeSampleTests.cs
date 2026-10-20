using System;
using System.Diagnostics;
using System.Threading;
using Int2Dds.Cdr;
using Int2Dds.Core;
using Int2Dds.Qos;
using Int2Dds.Tests.Helpers;
using Int2Dds.TypeInfo;
using Int2Dds.Types;
using Int2Dds.Xtypes;
using Xunit;

namespace Int2Dds.Tests
{
    /// <summary>
    /// Samples larger than the reader's initial 64 KB copy buffer must come through the
    /// batch (sample sequence) paths, not only the single-sample ones.
    /// </summary>
    public class LargeSampleTests
    {
        private const int LargeLength = 200 * 1024;

        private static void AwaitSamples(DataReader<HelloWorld> reader, int count)
        {
            var sw = Stopwatch.StartNew();
            while (true)
            {
                using (var loan = reader.ReadLoaned())
                    if (loan.Count == count) return;
                Assert.True(sw.Elapsed < TimeSpan.FromSeconds(5), "samples not cached within 5s");
                Thread.Sleep(10);
            }
        }

        [Fact]
        public void BatchReadAndTakeReturnSamplesLargerThanBuffer()
        {
            using (var dp = new DomainParticipant(0))
            using (var topic = dp.CreateTopic<HelloWorld>("LargeSampleBatchCs"))
            {
                var qos = new History(HistoryKind.KeepAll);
                var writer = dp.CreatePublisher().CreateDataWriter(topic, new DataWriterQos { History = qos });
                var reader = dp.CreateSubscriber().CreateDataReader(topic, new DataReaderQos { History = qos });
                var sw = Stopwatch.StartNew();
                while (reader.GetSubscriptionMatchedStatus().currentCount == 0)
                {
                    Assert.True(sw.Elapsed < TimeSpan.FromSeconds(5), "no match within 5s");
                    Thread.Sleep(10);
                }

                var large = new string('x', LargeLength);
                writer.Write(new HelloWorld(1, large));
                writer.Write(new HelloWorld(2, "small"));
                AwaitSamples(reader, 2);

                var read = reader.ReadBatch(10);
                Assert.Equal(2, read.Count);
                Assert.Equal(large, read[0].Sample.Data!.Message);

                var taken = reader.TakeBatch(10);
                Assert.Equal(2, taken.Count);
                Assert.Equal(1u, taken[0].Sample.Data!.Index);
                Assert.Equal(large, taken[0].Sample.Data!.Message);
                Assert.Equal("small", taken[1].Sample.Data!.Message);
            }
        }

        [Fact]
        public void DynamicTakeNextSampleReturnsSampleLargerThanBuffer()
        {
            using (var dp = new DomainParticipant(0))
            using (var b = new TypeInfoBuilder("LargeDynamicType", Extensibility.Final))
            {
                b.AddField("id", FieldType.Int32, true)
                 .AddField("label", FieldType.String);
                using (var topic = dp.CreateDynamicTopic("LargeSampleDynamicCs", b))
                using (var writer = dp.CreatePublisher().CreateDynamicDataWriter(topic))
                using (var reader = dp.CreateSubscriber().CreateDynamicDataReader(topic))
                {
                    var sw = Stopwatch.StartNew();
                    while (writer.MatchedReaders == 0 || reader.MatchedWriters == 0)
                    {
                        Assert.True(sw.Elapsed < TimeSpan.FromSeconds(5), "no match within 5s");
                        Thread.Sleep(10);
                    }

                    var w = new CdrWriter(Extensibility.Final, littleEndian: true, xcdr2: false);
                    w.WriteI32(1);
                    w.WriteString(new string('x', LargeLength));
                    var sent = w.ToBytes();
                    writer.WriteSerialized(sent);

                    byte[]? received = null;
                    sw.Restart();
                    while ((received = reader.TakeNextSample()) == null)
                    {
                        Assert.True(sw.Elapsed < TimeSpan.FromSeconds(5), "sample not received within 5s");
                        Thread.Sleep(10);
                    }
                    Assert.Equal(sent, received);
                    Assert.Null(reader.TakeNextSample());
                }
            }
        }
    }
}
