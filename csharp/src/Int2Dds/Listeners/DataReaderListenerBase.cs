namespace Int2Dds.Listeners
{
    /// <summary>
    /// Base class for DataReader listeners with virtual no-op implementations.
    /// Subclass this and override only the methods you need.
    /// </summary>
    public class DataReaderListenerBase : IDataReaderListener
    {
        /// <inheritdoc />
        public virtual void OnDataAvailable(object reader) { }

        /// <inheritdoc />
        public virtual void OnSubscriptionMatched(object reader, SubscriptionMatchedStatus status) { }

        /// <inheritdoc />
        public virtual void OnLivelinessChanged(object reader, LivelinessChangedStatus status) { }

        /// <inheritdoc />
        public virtual void OnRequestedDeadlineMissed(object reader, RequestedDeadlineMissedStatus status) { }

        /// <inheritdoc />
        public virtual void OnSampleLost(object reader, SampleLostStatus status) { }

        /// <inheritdoc />
        public virtual void OnSampleRejected(object reader, SampleRejectedStatus status) { }

        /// <inheritdoc />
        public virtual void OnRequestedIncompatibleQos(object reader, RequestedIncompatibleQosStatus status) { }
    }
}
