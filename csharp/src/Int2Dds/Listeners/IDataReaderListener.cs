namespace Int2Dds.Listeners
{
    /// <summary>
    /// Interface for DataReader listener callbacks.
    /// The reader parameter is the DataReader instance that triggered the event.
    /// Cast to DataReader&lt;T&gt; to access typed operations (e.g., Take, Read).
    /// </summary>
    public interface IDataReaderListener
    {
        /// <summary>Called when new data is available to read.</summary>
        void OnDataAvailable(object reader);

        /// <summary>Called when subscription matching status changes.</summary>
        void OnSubscriptionMatched(object reader, SubscriptionMatchedStatus status);

        /// <summary>Called when liveliness of matched writers changes.</summary>
        void OnLivelinessChanged(object reader, LivelinessChangedStatus status);

        /// <summary>Called when the requested deadline is missed.</summary>
        void OnRequestedDeadlineMissed(object reader, RequestedDeadlineMissedStatus status);

        /// <summary>Called when samples are lost.</summary>
        void OnSampleLost(object reader, SampleLostStatus status);

        /// <summary>Called when a sample is rejected.</summary>
        void OnSampleRejected(object reader, SampleRejectedStatus status);

        /// <summary>Called when incompatible QoS is detected.</summary>
        void OnRequestedIncompatibleQos(object reader, RequestedIncompatibleQosStatus status);
    }
}
