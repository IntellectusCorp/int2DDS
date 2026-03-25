using System;

namespace Int2Dds.Listeners
{
    /// <summary>
    /// Interface for DataWriter listener callbacks.
    /// </summary>
    public interface IDataWriterListener
    {
        /// <summary>Called when publication matching status changes.</summary>
        void OnPublicationMatched(IntPtr writerHandle, PublicationMatchedStatus status);

        /// <summary>Called when the writer misses its offered deadline.</summary>
        void OnOfferedDeadlineMissed(IntPtr writerHandle, OfferedDeadlineMissedStatus status);

        /// <summary>Called when the writer loses liveliness.</summary>
        void OnLivelinessLost(IntPtr writerHandle, LivelinessLostStatus status);

        /// <summary>Called when the writer detects incompatible QoS.</summary>
        void OnOfferedIncompatibleQos(IntPtr writerHandle, OfferedIncompatibleQosStatus status);
    }
}
