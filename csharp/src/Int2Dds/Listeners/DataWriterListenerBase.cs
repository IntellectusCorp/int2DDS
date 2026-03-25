namespace Int2Dds.Listeners
{
    /// <summary>
    /// Base class for DataWriter listeners with virtual no-op implementations.
    /// Subclass this and override only the methods you need.
    /// </summary>
    public class DataWriterListenerBase : IDataWriterListener
    {
        /// <inheritdoc />
        public virtual void OnPublicationMatched(object writer, PublicationMatchedStatus status) { }

        /// <inheritdoc />
        public virtual void OnOfferedDeadlineMissed(object writer, OfferedDeadlineMissedStatus status) { }

        /// <inheritdoc />
        public virtual void OnLivelinessLost(object writer, LivelinessLostStatus status) { }

        /// <inheritdoc />
        public virtual void OnOfferedIncompatibleQos(object writer, OfferedIncompatibleQosStatus status) { }
    }
}
