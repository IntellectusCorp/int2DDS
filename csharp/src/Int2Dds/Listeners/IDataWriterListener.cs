namespace Int2Dds.Listeners;

/// <summary>
/// Interface for DataWriter listener callbacks.
/// The writer parameter is the DataWriter instance that triggered the event.
/// Cast to DataWriter&lt;T&gt; to access typed operations (e.g., Write).
/// </summary>
public interface IDataWriterListener
{
    /// <summary>Called when publication matching status changes.</summary>
    void OnPublicationMatched(object writer, PublicationMatchedStatus status);

    /// <summary>Called when the writer misses its offered deadline.</summary>
    void OnOfferedDeadlineMissed(object writer, OfferedDeadlineMissedStatus status);

    /// <summary>Called when the writer loses liveliness.</summary>
    void OnLivelinessLost(object writer, LivelinessLostStatus status);

    /// <summary>Called when the writer detects incompatible QoS.</summary>
    void OnOfferedIncompatibleQos(object writer, OfferedIncompatibleQosStatus status);
}
