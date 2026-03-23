namespace Int2Dds.Qos;

public record TopicQos
{
    public Reliability? Reliability { get; init; }
    public Durability? Durability { get; init; }
    public History? History { get; init; }
    public Deadline? Deadline { get; init; }
    public Liveliness? Liveliness { get; init; }
    public DestinationOrder? DestinationOrder { get; init; }
    public ResourceLimits? ResourceLimits { get; init; }
    public TransportPriority? TransportPriority { get; init; }
    public Lifespan? Lifespan { get; init; }
    public Ownership? Ownership { get; init; }
    public DataRepresentation? DataRepresentation { get; init; }
}
