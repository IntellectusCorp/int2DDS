namespace Int2Dds.Listeners;

/// <summary>
/// Base class for DataReader listeners with virtual no-op implementations.
/// Subclass this and override only the methods you need.
/// </summary>
public class DataReaderListenerBase : IDataReaderListener
{
    /// <inheritdoc />
    public virtual void OnDataAvailable(nint readerHandle) { }

    /// <inheritdoc />
    public virtual void OnSubscriptionMatched(nint readerHandle, SubscriptionMatchedStatus status) { }

    /// <inheritdoc />
    public virtual void OnLivelinessChanged(nint readerHandle, LivelinessChangedStatus status) { }

    /// <inheritdoc />
    public virtual void OnRequestedDeadlineMissed(nint readerHandle, RequestedDeadlineMissedStatus status) { }

    /// <inheritdoc />
    public virtual void OnSampleLost(nint readerHandle, SampleLostStatus status) { }

    /// <inheritdoc />
    public virtual void OnSampleRejected(nint readerHandle, SampleRejectedStatus status) { }

    /// <inheritdoc />
    public virtual void OnRequestedIncompatibleQos(nint readerHandle, RequestedIncompatibleQosStatus status) { }
}
