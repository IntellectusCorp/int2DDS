namespace Int2Dds.Listeners;

/// <summary>
/// Interface for DataReader listener callbacks.
/// </summary>
public interface IDataReaderListener
{
    /// <summary>Called when new data is available to read.</summary>
    void OnDataAvailable(nint readerHandle);

    /// <summary>Called when subscription matching status changes.</summary>
    void OnSubscriptionMatched(nint readerHandle, SubscriptionMatchedStatus status);

    /// <summary>Called when liveliness of matched writers changes.</summary>
    void OnLivelinessChanged(nint readerHandle, LivelinessChangedStatus status);

    /// <summary>Called when the requested deadline is missed.</summary>
    void OnRequestedDeadlineMissed(nint readerHandle, RequestedDeadlineMissedStatus status);

    /// <summary>Called when samples are lost.</summary>
    void OnSampleLost(nint readerHandle, SampleLostStatus status);

    /// <summary>Called when a sample is rejected.</summary>
    void OnSampleRejected(nint readerHandle, SampleRejectedStatus status);

    /// <summary>Called when incompatible QoS is detected.</summary>
    void OnRequestedIncompatibleQos(nint readerHandle, RequestedIncompatibleQosStatus status);
}
