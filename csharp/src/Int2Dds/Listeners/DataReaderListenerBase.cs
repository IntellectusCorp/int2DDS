using System;

namespace Int2Dds.Listeners
{
    /// <summary>
    /// Base class for DataReader listeners with virtual no-op implementations.
    /// Subclass this and override only the methods you need.
    /// </summary>
    public class DataReaderListenerBase : IDataReaderListener
    {
        /// <inheritdoc />
        public virtual void OnDataAvailable(IntPtr readerHandle) { }

        /// <inheritdoc />
        public virtual void OnSubscriptionMatched(IntPtr readerHandle, SubscriptionMatchedStatus status) { }

        /// <inheritdoc />
        public virtual void OnLivelinessChanged(IntPtr readerHandle, LivelinessChangedStatus status) { }

        /// <inheritdoc />
        public virtual void OnRequestedDeadlineMissed(IntPtr readerHandle, RequestedDeadlineMissedStatus status) { }

        /// <inheritdoc />
        public virtual void OnSampleLost(IntPtr readerHandle, SampleLostStatus status) { }

        /// <inheritdoc />
        public virtual void OnSampleRejected(IntPtr readerHandle, SampleRejectedStatus status) { }

        /// <inheritdoc />
        public virtual void OnRequestedIncompatibleQos(IntPtr readerHandle, RequestedIncompatibleQosStatus status) { }
    }
}
