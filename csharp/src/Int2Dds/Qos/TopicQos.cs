namespace Int2Dds.Qos
{
    public class TopicQos
    {
        public Reliability? Reliability { get; set; }
        public Durability? Durability { get; set; }
        public History? History { get; set; }
        public Deadline? Deadline { get; set; }
        public Liveliness? Liveliness { get; set; }
        public DestinationOrder? DestinationOrder { get; set; }
        public ResourceLimits? ResourceLimits { get; set; }
        public TransportPriority? TransportPriority { get; set; }
        public Lifespan? Lifespan { get; set; }
        public Ownership? Ownership { get; set; }
        public DataRepresentation? DataRepresentation { get; set; }

        public TopicQos() { }
    }
}
