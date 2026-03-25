using System;
using System.Reflection;
using Int2Dds.Conditions;
using Int2Dds.Exceptions;
using Int2Dds.Interop;
using Int2Dds.Listeners;
using Int2Dds.Qos;
using Int2Dds.Types;

namespace Int2Dds.Core
{
    /// <summary>
    /// DataWriter - publishes data samples to a topic.
    ///
    /// DataWriters are created through Publisher.CreateDataWriter().
    /// </summary>
    /// <typeparam name="T">The DDS data type.</typeparam>
    public sealed class DataWriter<T> : IDisposable where T : class, IDdsType, new()
    {
        private static readonly bool s_hasKey =
            typeof(T).GetCustomAttribute<DdsTypeAttribute>()?.HasKey ?? false;

        private readonly IntPtr _handle;
        private readonly Topic<T> _topic;
        private bool _disposed;

        /// <summary>
        /// Creates a new DataWriter. Normally called via Publisher.CreateDataWriter.
        /// </summary>
        internal DataWriter(Publisher publisher, Topic<T> topic, DataWriterQos? qos = null,
            IDataWriterListener? listener = null, uint statusMask = 0)
        {
            _topic = topic;

            // Create QoS handle if provided
            IntPtr qosHandle = IntPtr.Zero;
            if (qos != null)
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_create_default(out qosHandle));
                try
                {
                    ApplyWriterQos(qosHandle, qos);
                }
                catch
                {
                    NativeMethods.int2dds_datawriter_qos_destroy(qosHandle);
                    throw;
                }
            }

            try
            {
                if (listener != null)
                {
                    // Listener support will be implemented separately
                    throw new NotImplementedException("DataWriter listener support is not yet implemented.");
                }
                else
                {
                    ReturnCodeHelper.CheckReturn(
                        NativeMethods.int2dds_create_datawriter(publisher.Handle, topic.Handle, qosHandle, out _handle));
                }
            }
            finally
            {
                if (qosHandle != IntPtr.Zero)
                    NativeMethods.int2dds_datawriter_qos_destroy(qosHandle);
            }
        }

        /// <summary>
        /// Gets the native handle. For internal use.
        /// </summary>
        internal IntPtr Handle => _handle;

        /// <summary>
        /// Gets the topic this writer publishes to.
        /// </summary>
        public Topic<T> Topic => _topic;

        /// <summary>
        /// Gets the current number of matched readers.
        /// </summary>
        public int MatchedReaders
        {
            get
            {
                var (_, currentCount) = GetPublicationMatchedStatus();
                return currentCount;
            }
        }

        /// <summary>
        /// Write a data sample.
        /// The sample is serialized using CDR and published to the topic.
        /// </summary>
        /// <param name="sample">The data sample to write.</param>
        public void Write(T sample)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

            var data = sample.SerializeCdr();
            var key = s_hasKey ? sample.SerializeKey() : null;

            unsafe
            {
                fixed (byte* pData = data)
                fixed (byte* pKey = key)
                {
                    ReturnCodeHelper.CheckReturn(
                        NativeMethods.int2dds_write_serialized(
                            _handle,
                            pData, (UIntPtr)data.Length,
                            pKey, key != null ? (UIntPtr)key.Length : UIntPtr.Zero));
                }
            }
        }

        /// <summary>
        /// Write a data sample with a specific source timestamp.
        /// </summary>
        /// <param name="sample">The data sample to write.</param>
        /// <param name="timestamp">The source timestamp to associate with the sample.</param>
        public void WriteWithTimestamp(T sample, DateTime timestamp)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

            var data = sample.SerializeCdr();
            var key = s_hasKey ? sample.SerializeKey() : null;

            var epoch = new DateTime(1970, 1, 1, 0, 0, 0, DateTimeKind.Utc);
            var elapsed = timestamp.ToUniversalTime() - epoch;
            var sec = (int)elapsed.TotalSeconds;
            var nanosec = (uint)((elapsed.TotalSeconds - sec) * 1_000_000_000);

            unsafe
            {
                fixed (byte* pData = data)
                fixed (byte* pKey = key)
                {
                    ReturnCodeHelper.CheckReturn(
                        NativeMethods.int2dds_write_serialized_w_timestamp(
                            _handle,
                            pData, (UIntPtr)data.Length,
                            pKey, key != null ? (UIntPtr)key.Length : UIntPtr.Zero,
                            sec, nanosec));
                }
            }
        }

        /// <summary>
        /// Register an instance and return its handle.
        /// </summary>
        /// <param name="sample">A data sample with key fields set.</param>
        /// <returns>An InstanceHandle for use in subsequent write/dispose/unregister calls.</returns>
        public InstanceHandle RegisterInstance(T sample)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

            var key = s_hasKey ? sample.SerializeKey() : null;
            if (key == null || key.Length == 0)
                return InstanceHandle.Nil;

            var handleBytes = new byte[16];

            unsafe
            {
                fixed (byte* pKey = key)
                fixed (byte* pHandle = handleBytes)
                {
                    ReturnCodeHelper.CheckReturn(
                        NativeMethods.int2dds_datawriter_register_instance(
                            _handle, pKey, (UIntPtr)key.Length, pHandle));
                }
            }

            return new InstanceHandle(handleBytes);
        }

        /// <summary>
        /// Unregister a previously registered instance.
        /// Informs readers that this writer will no longer modify the instance.
        /// </summary>
        /// <param name="sample">A data sample with key fields set.</param>
        /// <param name="handle">The InstanceHandle from RegisterInstance.</param>
        public void UnregisterInstance(T sample, InstanceHandle handle)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

            var key = s_hasKey ? sample.SerializeKey() : null;
            if (key == null || key.Length == 0)
                return;

            var handleBytes = handle.ToByteArray();

            unsafe
            {
                fixed (byte* pKey = key)
                fixed (byte* pHandle = handleBytes)
                {
                    ReturnCodeHelper.CheckReturn(
                        NativeMethods.int2dds_datawriter_unregister_instance(
                            _handle, pKey, (UIntPtr)key.Length, pHandle));
                }
            }
        }

        /// <summary>
        /// Dispose an instance, marking it as no longer valid.
        /// Readers will see the instance state change to NOT_ALIVE_DISPOSED.
        /// </summary>
        /// <param name="sample">A data sample with key fields set.</param>
        /// <param name="handle">The InstanceHandle from RegisterInstance.</param>
        public void DisposeInstance(T sample, InstanceHandle handle)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

            var key = s_hasKey ? sample.SerializeKey() : null;
            if (key == null || key.Length == 0)
                return;

            var handleBytes = handle.ToByteArray();

            unsafe
            {
                fixed (byte* pKey = key)
                fixed (byte* pHandle = handleBytes)
                {
                    ReturnCodeHelper.CheckReturn(
                        NativeMethods.int2dds_datawriter_dispose(
                            _handle, pKey, (UIntPtr)key.Length, pHandle));
                }
            }
        }

        /// <summary>
        /// Look up the handle of a previously registered instance.
        /// Does NOT register the instance.
        /// </summary>
        /// <param name="sample">A data sample with key fields set.</param>
        /// <returns>The InstanceHandle, or InstanceHandle.Nil if not found.</returns>
        public InstanceHandle LookupInstance(T sample)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

            var key = s_hasKey ? sample.SerializeKey() : null;
            if (key == null || key.Length == 0)
                return InstanceHandle.Nil;

            var handleBytes = new byte[16];

            unsafe
            {
                fixed (byte* pKey = key)
                fixed (byte* pHandle = handleBytes)
                {
                    ReturnCodeHelper.CheckReturn(
                        NativeMethods.int2dds_datawriter_lookup_instance(
                            _handle, pKey, (UIntPtr)key.Length, pHandle));
                }
            }

            return new InstanceHandle(handleBytes);
        }

        /// <summary>
        /// Gets the serialized key value for a given instance handle.
        /// </summary>
        /// <param name="handle">The instance handle.</param>
        /// <returns>The serialized key bytes.</returns>
        public byte[] GetKeyValue(InstanceHandle handle)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

            var handleBytes = handle.ToByteArray();
            var keyBuffer = new byte[256];

            unsafe
            {
                fixed (byte* pHandle = handleBytes)
                fixed (byte* pKey = keyBuffer)
                {
                    ReturnCodeHelper.CheckReturn(
                        NativeMethods.int2dds_datawriter_get_key_value(
                            _handle, pHandle, pKey, (UIntPtr)keyBuffer.Length, out var keySize));

                    var result = new byte[(int)keySize];
                    Array.Copy(keyBuffer, result, (int)keySize);
                    return result;
                }
            }
        }

        /// <summary>
        /// Assert liveliness for MANUAL_BY_TOPIC liveliness.
        /// </summary>
        public void AssertLiveliness()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_assert_liveliness(_handle));
        }

        /// <summary>
        /// Waits until all written data has been acknowledged by matched readers.
        /// </summary>
        /// <param name="timeout">Maximum time to wait.</param>
        public void WaitForAcknowledgments(TimeSpan timeout)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_datawriter_wait_for_acknowledgments(_handle, (long)timeout.TotalMilliseconds));
        }

        /// <summary>
        /// Gets the publication matched status.
        /// </summary>
        /// <returns>A tuple of (totalCount, currentCount) indicating matched readers.</returns>
        public (int totalCount, int currentCount) GetPublicationMatchedStatus()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_get_publication_matched_status(_handle, out var total, out var current));
            return (total, current);
        }

        /// <summary>
        /// Gets the liveliness lost status.
        /// </summary>
        public LivelinessLostStatus GetLivelinessLostStatus()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

            unsafe
            {
                NativeLivelinessLostStatus native;
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_datawriter_get_liveliness_lost_status(_handle, &native));
                return new LivelinessLostStatus(native.TotalCount, native.TotalCountChange);
            }
        }

        /// <summary>
        /// Gets the offered deadline missed status.
        /// </summary>
        public OfferedDeadlineMissedStatus GetOfferedDeadlineMissedStatus()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

            unsafe
            {
                NativeOfferedDeadlineMissedStatus native;
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_datawriter_get_offered_deadline_missed_status(_handle, &native));

                return new OfferedDeadlineMissedStatus(
                    native.TotalCount,
                    native.TotalCountChange);
            }
        }

        /// <summary>
        /// Gets the offered incompatible QoS status.
        /// </summary>
        public OfferedIncompatibleQosStatus GetOfferedIncompatibleQosStatus()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

            unsafe
            {
                NativeOfferedIncompatibleQosStatus native;
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_datawriter_get_offered_incompatible_qos_status(_handle, &native));
                return new OfferedIncompatibleQosStatus(
                    native.TotalCount,
                    native.TotalCountChange,
                    (int)native.LastPolicyId);
            }
        }

        /// <summary>
        /// Sets or replaces the listener for this DataWriter.
        /// </summary>
        /// <param name="listener">The listener to set, or null to remove.</param>
        /// <param name="statusMask">Bitmask of statuses to listen for.</param>
        public void SetListener(IDataWriterListener? listener, uint statusMask)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            // Listener infrastructure will be implemented separately
            throw new NotImplementedException("DataWriter listener support is not yet implemented.");
        }

        /// <summary>
        /// Gets the StatusCondition associated with this DataWriter.
        /// </summary>
        /// <returns>A StatusCondition for use with WaitSets.</returns>
        public StatusCondition GetStatusCondition()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_datawriter_get_statuscondition(_handle, out var conditionHandle));
            return new StatusCondition(conditionHandle);
        }

        private static void ApplyWriterQos(IntPtr qosHandle, DataWriterQos qos)
        {
            if (qos.Reliability != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_reliability(
                    qosHandle, (int)qos.Reliability.Kind, qos.Reliability.MaxBlockingTimeNs));

            if (qos.Durability != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_durability(
                    qosHandle, (int)qos.Durability.Kind));

            if (qos.History != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_history(
                    qosHandle, (int)qos.History.Kind, qos.History.Depth));

            if (qos.Ownership != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_ownership(
                    qosHandle, (int)qos.Ownership.Kind));

            if (qos.OwnershipStrength != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_ownership_strength(
                    qosHandle, qos.OwnershipStrength.Value));

            if (qos.ResourceLimits != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_resource_limits(
                    qosHandle, qos.ResourceLimits.MaxSamples, qos.ResourceLimits.MaxInstances,
                    qos.ResourceLimits.MaxSamplesPerInstance));

            if (qos.Lifespan != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_lifespan(
                    qosHandle, qos.Lifespan.DurationNs));

            if (qos.DestinationOrder != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_destination_order(
                    qosHandle, (int)qos.DestinationOrder.Kind));

            if (qos.LatencyBudget != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_latency_budget(
                    qosHandle, qos.LatencyBudget.DurationNs));

            if (qos.TransportPriority != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_transport_priority(
                    qosHandle, qos.TransportPriority.Value));

            if (qos.UserData != null && qos.UserData.Data != null && qos.UserData.Data.Length > 0)
            {
                unsafe
                {
                    fixed (byte* pData = qos.UserData.Data)
                    {
                        ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_user_data(
                            qosHandle, pData, (UIntPtr)qos.UserData.Data.Length));
                    }
                }
            }

            if (qos.WriterDataLifecycle != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_writer_data_lifecycle(
                    qosHandle, qos.WriterDataLifecycle.AutodisposeUnregisteredInstances));

            if (qos.DataRepresentation != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_data_representation(
                    qosHandle, (int)qos.DataRepresentation.Kind));

            if (qos.Deadline != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_deadline(
                    qosHandle, qos.Deadline.PeriodNs));

            if (qos.Liveliness != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_liveliness(
                    qosHandle, (int)qos.Liveliness.Kind, qos.Liveliness.LeaseDurationNs));
        }

        /// <summary>
        /// Releases all resources used by the DataWriter.
        /// </summary>
        public void Dispose()
        {
            if (_disposed) return;
            _disposed = true;
            NativeMethods.int2dds_delete_datawriter(_handle);
        }

        ~DataWriter()
        {
            if (!_disposed)
            {
                try { Dispose(); }
                catch { /* suppress errors during finalization */ }
            }
        }
    }
}
