namespace Int2Dds.Qos;

public record DataReaderQos
{
    public Reliability? Reliability { get; init; }
    public Durability? Durability { get; init; }
    public History? History { get; init; }
    public Ownership? Ownership { get; init; }
    public ResourceLimits? ResourceLimits { get; init; }
    public DestinationOrder? DestinationOrder { get; init; }
    public TimeBasedFilter? TimeBasedFilter { get; init; }
    public LatencyBudget? LatencyBudget { get; init; }
    public UserData? UserData { get; init; }
    public ReaderDataLifecycle? ReaderDataLifecycle { get; init; }
    public DataRepresentation? DataRepresentation { get; init; }
    public Deadline? Deadline { get; init; }
    public Liveliness? Liveliness { get; init; }
}
