using System;

namespace Int2Dds.Qos
{
    public class Reliability
    {
        public ReliabilityKind Kind { get; set; } = ReliabilityKind.Reliable;
        public TimeSpan? MaxBlockingTime { get; set; }

        public Reliability() { }
        public Reliability(ReliabilityKind kind = ReliabilityKind.Reliable, TimeSpan? maxBlockingTime = null)
        {
            Kind = kind;
            MaxBlockingTime = maxBlockingTime;
        }

        internal long MaxBlockingTimeNs => MaxBlockingTime.HasValue
            ? (long)(MaxBlockingTime.Value.TotalMilliseconds * 1_000_000)
            : 100_000_000L; // 100ms default
    }

    public class Durability
    {
        public DurabilityKind Kind { get; set; } = DurabilityKind.Volatile;

        public Durability() { }
        public Durability(DurabilityKind kind = DurabilityKind.Volatile)
        {
            Kind = kind;
        }
    }

    public class History
    {
        public HistoryKind Kind { get; set; } = HistoryKind.KeepLast;
        public int Depth { get; set; } = 1;

        public History() { }
        public History(HistoryKind kind = HistoryKind.KeepLast, int depth = 1)
        {
            Kind = kind;
            Depth = depth;
        }
    }

    public class Ownership
    {
        public OwnershipKind Kind { get; set; } = OwnershipKind.Shared;

        public Ownership() { }
        public Ownership(OwnershipKind kind = OwnershipKind.Shared)
        {
            Kind = kind;
        }
    }

    public class OwnershipStrength
    {
        public int Value { get; set; }

        public OwnershipStrength() { }
        public OwnershipStrength(int value = 0)
        {
            Value = value;
        }
    }

    public class ResourceLimits
    {
        public int MaxSamples { get; set; } = -1;
        public int MaxInstances { get; set; } = -1;
        public int MaxSamplesPerInstance { get; set; } = -1;

        public ResourceLimits() { }
        public ResourceLimits(int maxSamples = -1, int maxInstances = -1, int maxSamplesPerInstance = -1)
        {
            MaxSamples = maxSamples;
            MaxInstances = maxInstances;
            MaxSamplesPerInstance = maxSamplesPerInstance;
        }
    }

    public class Lifespan
    {
        public TimeSpan? Duration { get; set; }

        public Lifespan() { }
        public Lifespan(TimeSpan? duration = null)
        {
            Duration = duration;
        }

        internal long DurationNs => Duration.HasValue
            ? (long)(Duration.Value.TotalMilliseconds * 1_000_000)
            : long.MaxValue;
    }

    public class DestinationOrder
    {
        public DestinationOrderKind Kind { get; set; } = DestinationOrderKind.ByReception;

        public DestinationOrder() { }
        public DestinationOrder(DestinationOrderKind kind = DestinationOrderKind.ByReception)
        {
            Kind = kind;
        }
    }

    public class LatencyBudget
    {
        public TimeSpan? Duration { get; set; }

        public LatencyBudget() { }
        public LatencyBudget(TimeSpan? duration = null)
        {
            Duration = duration;
        }

        internal long DurationNs => Duration.HasValue
            ? (long)(Duration.Value.TotalMilliseconds * 1_000_000)
            : 0;
    }

    public class TransportPriority
    {
        public int Value { get; set; }

        public TransportPriority() { }
        public TransportPriority(int value = 0)
        {
            Value = value;
        }
    }

    public class UserData
    {
        public byte[] Data { get; set; } = Array.Empty<byte>();

        public UserData() { }
        public UserData(byte[] data)
        {
            Data = data;
        }
    }

    public class WriterDataLifecycle
    {
        public bool AutodisposeUnregisteredInstances { get; set; } = true;

        public WriterDataLifecycle() { }
        public WriterDataLifecycle(bool autodisposeUnregisteredInstances = true)
        {
            AutodisposeUnregisteredInstances = autodisposeUnregisteredInstances;
        }
    }

    public class ReaderDataLifecycle
    {
        public TimeSpan? AutopurgeNowriterSamplesDelay { get; set; }
        public TimeSpan? AutopurgeDisposedSamplesDelay { get; set; }

        public ReaderDataLifecycle() { }
        public ReaderDataLifecycle(TimeSpan? autopurgeNowriterSamplesDelay = null, TimeSpan? autopurgeDisposedSamplesDelay = null)
        {
            AutopurgeNowriterSamplesDelay = autopurgeNowriterSamplesDelay;
            AutopurgeDisposedSamplesDelay = autopurgeDisposedSamplesDelay;
        }

        internal long AutopurgeNowriterNs => AutopurgeNowriterSamplesDelay.HasValue
            ? (long)(AutopurgeNowriterSamplesDelay.Value.TotalMilliseconds * 1_000_000)
            : long.MaxValue;

        internal long AutopurgeDisposedNs => AutopurgeDisposedSamplesDelay.HasValue
            ? (long)(AutopurgeDisposedSamplesDelay.Value.TotalMilliseconds * 1_000_000)
            : long.MaxValue;
    }

    public class DataRepresentation
    {
        public DataRepresentationKind Kind { get; set; } = DataRepresentationKind.Xcdr1;

        public DataRepresentation() { }
        public DataRepresentation(DataRepresentationKind kind = DataRepresentationKind.Xcdr1)
        {
            Kind = kind;
        }
    }

    public class TimeBasedFilter
    {
        public TimeSpan? MinimumSeparation { get; set; }

        public TimeBasedFilter() { }
        public TimeBasedFilter(TimeSpan? minimumSeparation = null)
        {
            MinimumSeparation = minimumSeparation;
        }

        internal long MinimumSeparationNs => MinimumSeparation.HasValue
            ? (long)(MinimumSeparation.Value.TotalMilliseconds * 1_000_000)
            : 0;
    }

    public class Deadline
    {
        public TimeSpan? Period { get; set; }

        public Deadline() { }
        public Deadline(TimeSpan? period = null)
        {
            Period = period;
        }

        internal long PeriodNs => Period.HasValue
            ? (long)(Period.Value.TotalMilliseconds * 1_000_000)
            : long.MaxValue;
    }

    public class Liveliness
    {
        public LivelinessKind Kind { get; set; } = LivelinessKind.Automatic;
        public TimeSpan? LeaseDuration { get; set; }

        public Liveliness() { }
        public Liveliness(LivelinessKind kind = LivelinessKind.Automatic, TimeSpan? leaseDuration = null)
        {
            Kind = kind;
            LeaseDuration = leaseDuration;
        }

        internal long LeaseDurationNs => LeaseDuration.HasValue
            ? (long)(LeaseDuration.Value.TotalMilliseconds * 1_000_000)
            : long.MaxValue;
    }

    public class Partition
    {
        public string[] Names { get; set; } = Array.Empty<string>();

        public Partition() { }
        public Partition(string[] names)
        {
            Names = names;
        }
    }

    /// <summary>
    /// PropertyQosPolicy entries (DomainParticipant only).
    /// The well-known name <c>int2dds.transport.UDPv4.multicast_ttl</c> configures
    /// the IPv4 multicast TTL for the participant; use <see cref="SetMulticastTtl"/>
    /// as a convenience.
    /// </summary>
    public class Property
    {
        public const string MulticastTtlName = "int2dds.transport.UDPv4.multicast_ttl";

        public System.Collections.Generic.List<PropertyEntry> Entries { get; }
            = new System.Collections.Generic.List<PropertyEntry>();

        public Property() { }

        public void Add(string name, string value, bool propagate = true)
        {
            Entries.Add(new PropertyEntry(name, value, propagate));
        }

        public void SetMulticastTtl(byte ttl)
        {
            Entries.RemoveAll(e => e.Name == MulticastTtlName);
            Entries.Add(new PropertyEntry(MulticastTtlName, ttl.ToString(), false));
        }
    }

    public sealed class PropertyEntry
    {
        public string Name { get; }
        public string Value { get; }
        public bool Propagate { get; }

        public PropertyEntry(string name, string value, bool propagate = true)
        {
            Name = name;
            Value = value;
            Propagate = propagate;
        }
    }
}
