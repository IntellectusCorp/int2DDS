namespace Int2Dds.Core;

/// <summary>
/// Metadata associated with a received data sample.
/// </summary>
public readonly record struct SampleInfo
{
    /// <summary>Source timestamp (seconds component).</summary>
    public int SourceTimestampSec { get; init; }

    /// <summary>Source timestamp (nanoseconds component).</summary>
    public uint SourceTimestampNanosec { get; init; }

    /// <summary>Sample state bitmask (Read / NotRead).</summary>
    public uint SampleState { get; init; }

    /// <summary>View state bitmask (New / NotNew).</summary>
    public uint ViewState { get; init; }

    /// <summary>Instance state bitmask (Alive / NotAliveDisposed / NotAliveNoWriters).</summary>
    public uint InstanceState { get; init; }

    /// <summary>Handle identifying the data instance.</summary>
    public InstanceHandle InstanceHandle { get; init; }

    /// <summary>Handle identifying the publication that wrote this sample.</summary>
    public InstanceHandle PublicationHandle { get; init; }

    /// <summary>Number of times the instance has been disposed.</summary>
    public int DisposedGenerationCount { get; init; }

    /// <summary>Number of times the instance has had zero writers.</summary>
    public int NoWritersGenerationCount { get; init; }

    /// <summary>Rank of this sample within the instance.</summary>
    public int SampleRank { get; init; }

    /// <summary>Generation rank of this sample.</summary>
    public int GenerationRank { get; init; }

    /// <summary>Absolute generation rank of this sample.</summary>
    public int AbsoluteGenerationRank { get; init; }

    /// <summary>True if this sample contains valid data.</summary>
    public bool ValidData { get; init; }

    /// <summary>
    /// Reconstructs the source timestamp as a DateTimeOffset.
    /// </summary>
    public DateTimeOffset SourceTimestamp =>
        DateTimeOffset.UnixEpoch.AddSeconds(SourceTimestampSec).AddTicks(SourceTimestampNanosec / 100);
}
