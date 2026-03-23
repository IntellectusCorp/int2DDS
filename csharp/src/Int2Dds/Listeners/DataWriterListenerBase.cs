namespace Int2Dds.Listeners;

/// <summary>
/// Base class for DataWriter listeners with virtual no-op implementations.
/// Subclass this and override only the methods you need.
/// </summary>
public class DataWriterListenerBase : IDataWriterListener
{
    /// <inheritdoc />
    public virtual void OnPublicationMatched(nint writerHandle, PublicationMatchedStatus status) { }

    /// <inheritdoc />
    public virtual void OnOfferedDeadlineMissed(nint writerHandle, OfferedDeadlineMissedStatus status) { }

    /// <inheritdoc />
    public virtual void OnLivelinessLost(nint writerHandle, LivelinessLostStatus status) { }

    /// <inheritdoc />
    public virtual void OnOfferedIncompatibleQos(nint writerHandle, OfferedIncompatibleQosStatus status) { }
}
