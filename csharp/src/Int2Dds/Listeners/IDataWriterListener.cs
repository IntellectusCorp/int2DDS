namespace Int2Dds.Listeners;

/// <summary>
/// Interface for DataWriter listener callbacks.
/// </summary>
public interface IDataWriterListener
{
    /// <summary>Called when publication matching status changes.</summary>
    void OnPublicationMatched(nint writerHandle, PublicationMatchedStatus status);

    /// <summary>Called when the writer misses its offered deadline.</summary>
    void OnOfferedDeadlineMissed(nint writerHandle, OfferedDeadlineMissedStatus status);

    /// <summary>Called when the writer loses liveliness.</summary>
    void OnLivelinessLost(nint writerHandle, LivelinessLostStatus status);

    /// <summary>Called when the writer detects incompatible QoS.</summary>
    void OnOfferedIncompatibleQos(nint writerHandle, OfferedIncompatibleQosStatus status);
}
