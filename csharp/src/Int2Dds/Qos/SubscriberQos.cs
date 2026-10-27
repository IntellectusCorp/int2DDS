namespace Int2Dds.Qos
{
    public class SubscriberQos
    {
        public Partition? Partition { get; set; }
        public Presentation? Presentation { get; set; }
        public GroupData? GroupData { get; set; }

        public SubscriberQos() { }
    }
}
