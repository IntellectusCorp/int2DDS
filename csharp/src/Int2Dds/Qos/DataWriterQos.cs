namespace Int2Dds.Qos
{
    public class DataWriterQos
    {
        public Reliability? Reliability { get; set; }
        public Durability? Durability { get; set; }
        public History? History { get; set; }
        public Ownership? Ownership { get; set; }
        public OwnershipStrength? OwnershipStrength { get; set; }
        public ResourceLimits? ResourceLimits { get; set; }
        public Lifespan? Lifespan { get; set; }
        public DestinationOrder? DestinationOrder { get; set; }
        public LatencyBudget? LatencyBudget { get; set; }
        public TransportPriority? TransportPriority { get; set; }
        public UserData? UserData { get; set; }
        public WriterDataLifecycle? WriterDataLifecycle { get; set; }
        public DataRepresentation? DataRepresentation { get; set; }
        public Deadline? Deadline { get; set; }
        public Liveliness? Liveliness { get; set; }

        /// <summary>DATA_FRAG max fragment size in bytes (int2DDS extension).</summary>
        public int? DataFrag { get; set; }

        public DataWriterQos() { }
    }
}
