using System;
using Int2Dds.Qos;
using Xunit;

namespace Int2Dds.Tests
{
    public class QosTests
    {
        [Fact]
        public void Reliability_DefaultIsReliable()
        {
            var r = new Reliability();
            Assert.Equal(ReliabilityKind.Reliable, r.Kind);
            Assert.Equal(100_000_000L, r.MaxBlockingTimeNs); // 100ms
        }

        [Fact]
        public void Reliability_BestEffort()
        {
            var r = new Reliability(ReliabilityKind.BestEffort);
            Assert.Equal(ReliabilityKind.BestEffort, r.Kind);
        }

        [Fact]
        public void Reliability_CustomBlockingTime()
        {
            var r = new Reliability { MaxBlockingTime = TimeSpan.FromSeconds(1) };
            Assert.Equal(1_000_000_000L, r.MaxBlockingTimeNs);
        }

        [Fact]
        public void Durability_DefaultIsVolatile()
        {
            var d = new Durability();
            Assert.Equal(DurabilityKind.Volatile, d.Kind);
        }

        [Fact]
        public void History_DefaultIsKeepLast1()
        {
            var h = new History();
            Assert.Equal(HistoryKind.KeepLast, h.Kind);
            Assert.Equal(1, h.Depth);
        }

        [Fact]
        public void History_KeepAll()
        {
            var h = new History(HistoryKind.KeepAll);
            Assert.Equal(HistoryKind.KeepAll, h.Kind);
        }

        [Fact]
        public void Ownership_DefaultIsShared()
        {
            var o = new Ownership();
            Assert.Equal(OwnershipKind.Shared, o.Kind);
        }

        [Fact]
        public void ResourceLimits_DefaultIsUnlimited()
        {
            var rl = new ResourceLimits();
            Assert.Equal(-1, rl.MaxSamples);
            Assert.Equal(-1, rl.MaxInstances);
            Assert.Equal(-1, rl.MaxSamplesPerInstance);
        }

        [Fact]
        public void Deadline_DefaultIsInfinite()
        {
            var d = new Deadline();
            Assert.Equal(long.MaxValue, d.PeriodNs);
        }

        [Fact]
        public void Deadline_CustomPeriod()
        {
            var d = new Deadline(TimeSpan.FromMilliseconds(500));
            Assert.Equal(500_000_000L, d.PeriodNs);
        }

        [Fact]
        public void Liveliness_DefaultIsAutomatic()
        {
            var l = new Liveliness();
            Assert.Equal(LivelinessKind.Automatic, l.Kind);
            Assert.Equal(long.MaxValue, l.LeaseDurationNs);
        }

        [Fact]
        public void Partition_DefaultIsEmpty()
        {
            var p = new Partition();
            Assert.Empty(p.Names);
        }

        [Fact]
        public void Partition_WithNames()
        {
            var p = new Partition(new string[] { "part1", "part2" });
            Assert.Equal(2, p.Names.Length);
            Assert.Equal("part1", p.Names[0]);
        }

        [Fact]
        public void DataWriterQos_DefaultHasNullPolicies()
        {
            var qos = new DataWriterQos();
            Assert.Null(qos.Reliability);
            Assert.Null(qos.Durability);
            Assert.Null(qos.History);
        }

        [Fact]
        public void DataWriterQos_WithInit()
        {
            var qos = new DataWriterQos
            {
                Reliability = new Reliability(ReliabilityKind.BestEffort),
                History = new History(HistoryKind.KeepAll),
            };
            Assert.Equal(ReliabilityKind.BestEffort, qos.Reliability.Kind);
            Assert.Equal(HistoryKind.KeepAll, qos.History.Kind);
        }

        [Fact]
        public void DataReaderQos_WithTimeBasedFilter()
        {
            var qos = new DataReaderQos
            {
                TimeBasedFilter = new TimeBasedFilter(TimeSpan.FromMilliseconds(100)),
            };
            Assert.Equal(100_000_000L, qos.TimeBasedFilter.MinimumSeparationNs);
        }

        [Fact]
        public void TopicQos_WithAllPolicies()
        {
            var qos = new TopicQos
            {
                Reliability = new Reliability(ReliabilityKind.Reliable),
                Durability = new Durability(DurabilityKind.TransientLocal),
                History = new History(HistoryKind.KeepAll),
            };
            Assert.NotNull(qos.Reliability);
            Assert.NotNull(qos.Durability);
            Assert.NotNull(qos.History);
        }

        [Fact]
        public void UserData_DefaultIsEmpty()
        {
            var ud = new UserData();
            Assert.Empty(ud.Data);
        }

        [Fact]
        public void WriterDataLifecycle_DefaultIsTrue()
        {
            var wdl = new WriterDataLifecycle();
            Assert.True(wdl.AutodisposeUnregisteredInstances);
        }

        [Fact]
        public void DataRepresentation_DefaultIsXcdr2()
        {
            var dr = new DataRepresentation();
            Assert.Equal(DataRepresentationKind.Xcdr2, dr.Kind);
        }
    }
}
