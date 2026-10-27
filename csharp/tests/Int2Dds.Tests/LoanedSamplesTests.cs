using System;
using System.Diagnostics;
using System.Threading;
using Int2Dds.Cdr;
using Int2Dds.Core;
using Int2Dds.Exceptions;
using Int2Dds.Interop;
using Int2Dds.Qos;
using Int2Dds.Types;
using Xunit;

namespace Int2Dds.Tests
{
    /// <summary>IDL struct: LoanKeyed { @key unsigned long index; string message; }; FINAL.</summary>
    [DdsType("LoanKeyed", 0, true)]
    public class LoanKeyed : IDdsType
    {
        public static readonly DdsTypeInfoField[] DdsTypeInfoFields = new DdsTypeInfoField[]
        {
            new DdsTypeInfoField("field", "index", 9, 0u, 1),
            new DdsTypeInfoField("string", "message", 0, 0u, 0),
        };

        public uint Index { get; set; }
        public string Message { get; set; } = "";

        public LoanKeyed() { }

        public LoanKeyed(uint index, string message)
        {
            Index = index;
            Message = message;
        }

        public byte[] SerializeCdr() => SerializeCdr(false);

        public byte[] SerializeCdr(bool xcdr2)
        {
            var w = new CdrWriter(Extensibility.Final);
            w.WriteU32(Index);
            w.WriteString(Message);
            return w.ToBytes();
        }
    }

    public class LoanedSamplesTests
    {
        private static void AwaitMatch(DataReader<LoanKeyed> reader)
        {
            var sw = Stopwatch.StartNew();
            while (reader.GetSubscriptionMatchedStatus().currentCount == 0)
            {
                Assert.True(sw.Elapsed < TimeSpan.FromSeconds(5), "no match within 5s");
                Thread.Sleep(10);
            }
        }

        private static void AwaitSamples(DataReader<LoanKeyed> reader, int count)
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

        private static uint Index(LoanedSample s) => new CdrReader(s.SerializedData.ToArray()).ReadU32();

        /// <summary>Lend, refuse a foreign return, block deletion, return, then delete.</summary>
        [Fact]
        public void LoanScenario()
        {
            using (var dp = new DomainParticipant(0))
            using (var topic = dp.CreateTopic<LoanKeyed>("LoanedScenarioCs"))
            {
                var writer = dp.CreatePublisher().CreateDataWriter(
                    topic, new DataWriterQos { History = new History(HistoryKind.KeepAll) });
                var sub = dp.CreateSubscriber();
                var readerQos = new DataReaderQos { History = new History(HistoryKind.KeepAll) };
                var reader = sub.CreateDataReader(topic, readerQos);
                var other = sub.CreateDataReader(topic, readerQos);
                AwaitMatch(reader);
                AwaitMatch(other);
                writer.Write(new LoanKeyed(1, "a"));
                writer.Write(new LoanKeyed(2, "b"));
                AwaitSamples(reader, 2);

                var rc = reader.CreateReadCondition();

                // Instance variants lend one instance: by handle, or iterating from Nil.
                InstanceHandle firstHandle, secondHandle;
                using (var all = reader.ReadLoaned())
                {
                    firstHandle = all[0].Info.InstanceHandle;
                    secondHandle = all[1].Info.InstanceHandle;
                }
                using (var byHandle = reader.ReadInstanceLoaned(secondHandle))
                {
                    Assert.Single(byHandle);
                    Assert.Equal(2u, Index(byHandle[0]));
                }
                using (var next = reader.ReadNextInstanceLoaned(InstanceHandle.Nil))
                {
                    Assert.Single(next);
                    Assert.Equal(1u, Index(next[0]));
                }
                using (var next = reader.ReadNextInstanceWithConditionLoaned(firstHandle, rc))
                {
                    Assert.Single(next);
                    Assert.Equal(2u, Index(next[0]));
                }

                var loan = reader.TakeWithConditionLoaned(rc);
                Assert.Equal(2, loan.Count);
                var first = loan[0];
                Assert.True(first.Info.ValidData);
                Assert.Equal(1u, Index(first));
                Assert.Equal(2u, Index(loan[1]));

                Assert.Throws<DdsPreconditionNotMetException>(() => other.ReturnLoan(loan));
                Assert.Equal(1u, Index(first));
                rc.Dispose();
                Assert.Equal(ReturnCode.PreconditionNotMet, NativeMethods.int2dds_delete_datareader(reader.Handle));

                loan.Dispose();
                loan.Dispose();
                Assert.Throws<ObjectDisposedException>(() => first.SerializedData.Length);
                Assert.Throws<ObjectDisposedException>(() => loan.Count);
                using (var empty = reader.ReadLoaned())
                    Assert.Empty(empty);
                reader.Dispose();
            }
        }
    }
}
