using System;

namespace Int2Dds.Listeners
{
    /// <summary>
    /// Base class for DataWriter listeners with virtual no-op implementations.
    /// Subclass this and override only the methods you need.
    /// </summary>
    public class DataWriterListenerBase : IDataWriterListener
    {
        /// <inheritdoc />
        public virtual void OnPublicationMatched(IntPtr writerHandle, PublicationMatchedStatus status) { }

        /// <inheritdoc />
        public virtual void OnOfferedDeadlineMissed(IntPtr writerHandle, OfferedDeadlineMissedStatus status) { }

        /// <inheritdoc />
        public virtual void OnLivelinessLost(IntPtr writerHandle, LivelinessLostStatus status) { }

        /// <inheritdoc />
        public virtual void OnOfferedIncompatibleQos(IntPtr writerHandle, OfferedIncompatibleQosStatus status) { }
    }
}
