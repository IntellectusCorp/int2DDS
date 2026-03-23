namespace Int2Dds.Qos;

public record DataWriterQos
{
    public Reliability? Reliability { get; init; }
    public Durability? Durability { get; init; }
    public History? History { get; init; }
    public Ownership? Ownership { get; init; }
    public OwnershipStrength? OwnershipStrength { get; init; }
    public ResourceLimits? ResourceLimits { get; init; }
    public Lifespan? Lifespan { get; init; }
    public DestinationOrder? DestinationOrder { get; init; }
    public LatencyBudget? LatencyBudget { get; init; }
    public TransportPriority? TransportPriority { get; init; }
    public UserData? UserData { get; init; }
    public WriterDataLifecycle? WriterDataLifecycle { get; init; }
    public DataRepresentation? DataRepresentation { get; init; }
    public Deadline? Deadline { get; init; }
    public Liveliness? Liveliness { get; init; }
}
