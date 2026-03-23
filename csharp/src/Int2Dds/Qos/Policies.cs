namespace Int2Dds.Qos;

public record Reliability(ReliabilityKind Kind = ReliabilityKind.Reliable, TimeSpan? MaxBlockingTime = null)
{
    internal long MaxBlockingTimeNs => MaxBlockingTime.HasValue
        ? (long)(MaxBlockingTime.Value.TotalMilliseconds * 1_000_000)
        : 100_000_000L; // 100ms default
}

public record Durability(DurabilityKind Kind = DurabilityKind.Volatile);
public record History(HistoryKind Kind = HistoryKind.KeepLast, int Depth = 1);
public record Ownership(OwnershipKind Kind = OwnershipKind.Shared);
public record OwnershipStrength(int Value = 0);
public record ResourceLimits(int MaxSamples = -1, int MaxInstances = -1, int MaxSamplesPerInstance = -1);

public record Lifespan(TimeSpan? Duration = null)
{
    internal long DurationNs => Duration.HasValue
        ? (long)(Duration.Value.TotalMilliseconds * 1_000_000)
        : long.MaxValue;
}

public record DestinationOrder(DestinationOrderKind Kind = DestinationOrderKind.ByReception);

public record LatencyBudget(TimeSpan? Duration = null)
{
    internal long DurationNs => Duration.HasValue
        ? (long)(Duration.Value.TotalMilliseconds * 1_000_000)
        : 0;
}

public record TransportPriority(int Value = 0);

public record UserData(byte[] Data)
{
    public UserData() : this(Array.Empty<byte>()) { }
}

public record WriterDataLifecycle(bool AutodisposeUnregisteredInstances = true);

public record ReaderDataLifecycle(
    TimeSpan? AutopurgeNowriterSamplesDelay = null,
    TimeSpan? AutopurgeDisposedSamplesDelay = null)
{
    internal long AutopurgeNowriterNs => AutopurgeNowriterSamplesDelay.HasValue
        ? (long)(AutopurgeNowriterSamplesDelay.Value.TotalMilliseconds * 1_000_000)
        : long.MaxValue;

    internal long AutopurgeDisposedNs => AutopurgeDisposedSamplesDelay.HasValue
        ? (long)(AutopurgeDisposedSamplesDelay.Value.TotalMilliseconds * 1_000_000)
        : long.MaxValue;
}

public record DataRepresentation(DataRepresentationKind Kind = DataRepresentationKind.Xcdr2);

public record TimeBasedFilter(TimeSpan? MinimumSeparation = null)
{
    internal long MinimumSeparationNs => MinimumSeparation.HasValue
        ? (long)(MinimumSeparation.Value.TotalMilliseconds * 1_000_000)
        : 0;
}

public record Deadline(TimeSpan? Period = null)
{
    internal long PeriodNs => Period.HasValue
        ? (long)(Period.Value.TotalMilliseconds * 1_000_000)
        : long.MaxValue;
}

public record Liveliness(LivelinessKind Kind = LivelinessKind.Automatic, TimeSpan? LeaseDuration = null)
{
    internal long LeaseDurationNs => LeaseDuration.HasValue
        ? (long)(LeaseDuration.Value.TotalMilliseconds * 1_000_000)
        : long.MaxValue;
}

public record Partition(string[] Names)
{
    public Partition() : this(Array.Empty<string>()) { }
}
