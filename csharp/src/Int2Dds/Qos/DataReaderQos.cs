namespace Int2Dds.Qos
{
    public class DataReaderQos
    {
        public Reliability? Reliability { get; set; }
        public Durability? Durability { get; set; }
        public History? History { get; set; }
        public Ownership? Ownership { get; set; }
        public ResourceLimits? ResourceLimits { get; set; }
        public DestinationOrder? DestinationOrder { get; set; }
        public LifespanReference? LifespanReference { get; set; }
        public TimeBasedFilter? TimeBasedFilter { get; set; }
        public LatencyBudget? LatencyBudget { get; set; }
        public UserData? UserData { get; set; }
        public ReaderDataLifecycle? ReaderDataLifecycle { get; set; }
        public DataRepresentation? DataRepresentation { get; set; }
        public Deadline? Deadline { get; set; }
        public Liveliness? Liveliness { get; set; }

        public DataReaderQos() { }
    }
}
