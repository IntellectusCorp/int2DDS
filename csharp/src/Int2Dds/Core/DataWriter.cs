using System;
using System.Reflection;
using System.Text;
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
        private readonly bool _xcdr2;
        private IntPtr _listenerContextHandle;
        private bool _disposed;

        /// <summary>
        /// Creates a new DataWriter. Normally called via Publisher.CreateDataWriter.
        /// </summary>
        internal DataWriter(Publisher publisher, Topic<T> topic, DataWriterQos qos = null,
            IDataWriterListener listener = null, uint statusMask = 0)
        {
            _topic = topic;

            // Effective representation = caller's choice, else the core default
            // (single source of truth in the Rust core, not hardcoded here).
            int effectiveRepr = qos?.DataRepresentation != null
                ? (int)qos.DataRepresentation.Kind
                : NativeMethods.int2dds_default_data_representation();
            _xcdr2 = effectiveRepr == (int)Qos.DataRepresentationKind.Xcdr2;

            // Always create a QoS handle so that the native layer receives the
            // correct DataRepresentation default even when the caller does not
            // supply an explicit QoS object.
            IntPtr qosHandle = IntPtr.Zero;
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_create_default(out qosHandle));
            try
            {
                if (qos != null)
                    ApplyWriterQos(qosHandle, qos);

                // Ensure SEDP advertises the same encoding that C# actually uses.
                if (qos?.DataRepresentation == null)
                    ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_data_representation(
                        qosHandle, effectiveRepr));
            }
            catch
            {
                NativeMethods.int2dds_datawriter_qos_destroy(qosHandle);
                throw;
            }

            try
            {
                if (listener != null)
                {
                    unsafe
                    {
                        var (nativeListener, contextHandle) = ListenerRegistry.CreateWriterListener(listener, this);
                        _listenerContextHandle = contextHandle;
                        try
                        {
                            ReturnCodeHelper.CheckReturn(
                                NativeMethods.int2dds_create_datawriter_with_listener(
                                    publisher.Handle, topic.Handle, qosHandle, &nativeListener, statusMask, out _handle));
                        }
                        catch
                        {
                            ListenerRegistry.FreeListener(_listenerContextHandle);
                            _listenerContextHandle = IntPtr.Zero;
                            throw;
                        }
                    }
                }
                else
                {
                    ReturnCodeHelper.CheckReturn(
                        NativeMethods.int2dds_create_datawriter(publisher.Handle, topic.Handle, qosHandle, out _handle));
                }
            }
            finally
            {
                NativeMethods.int2dds_datawriter_qos_destroy(qosHandle);
            }
        }

        /// <summary>
        /// Creates a new DataWriter using a QoS profile path.
        /// Normally called via Publisher.CreateDataWriterWithProfile.
        /// </summary>
        internal DataWriter(Publisher publisher, Topic<T> topic, string qosPath,
            IDataWriterListener listener = null, uint statusMask = 0)
        {
            _topic = topic;
            _xcdr2 = false;

            unsafe
            {
                var qosPathBytes = Encoding.UTF8.GetBytes(qosPath + '\0');
                fixed (byte* pQos = qosPathBytes)
                {
                    if (listener != null)
                    {
                        var (nativeListener, contextHandle) = ListenerRegistry.CreateWriterListener(listener, this);
                        _listenerContextHandle = contextHandle;
                        try
                        {
                            ReturnCodeHelper.CheckReturn(
                                NativeMethods.int2dds_create_datawriter_with_profile_and_listener(
                                    publisher.Handle, topic.Handle, pQos, &nativeListener, statusMask, out _handle));
                        }
                        catch
                        {
                            ListenerRegistry.FreeListener(_listenerContextHandle);
                            _listenerContextHandle = IntPtr.Zero;
                            throw;
                        }
                    }
                    else
                    {
                        ReturnCodeHelper.CheckReturn(
                            NativeMethods.int2dds_create_datawriter_with_profile(
                                publisher.Handle, topic.Handle, pQos, out _handle));
                    }
                }
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

            var data = sample.SerializeCdr(_xcdr2);
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

            var data = sample.SerializeCdr(_xcdr2);
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
        /// </summary>
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
        /// </summary>
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
        /// </summary>
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
        public byte[] GetKeyValue(InstanceHandle handle)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

            var handleBytes = handle.ToByteArray();
            var keyBuffer = new byte[256];

            unsafe
            {
                fixed (byte* pHandle = handleBytes)
                {
                    UIntPtr keySize;
                    fixed (byte* pKey = keyBuffer)
                    {
                        var ret = NativeMethods.int2dds_datawriter_get_key_value(
                            _handle, pHandle, pKey, (UIntPtr)keyBuffer.Length, out keySize);

                        if (ret == ReturnCode.Ok)
                        {
                            var result = new byte[(int)keySize];
                            Array.Copy(keyBuffer, result, (int)keySize);
                            return result;
                        }

                        // Buffer too small — retry with required size
                        if ((int)keySize > keyBuffer.Length)
                        {
                            keyBuffer = new byte[(int)keySize];
                        }
                        else
                        {
                            ReturnCodeHelper.CheckReturn(ret);
                        }
                    }

                    fixed (byte* pKey = keyBuffer)
                    {
                        ReturnCodeHelper.CheckReturn(
                            NativeMethods.int2dds_datawriter_get_key_value(
                                _handle, pHandle, pKey, (UIntPtr)keyBuffer.Length, out keySize));

                        var result = new byte[(int)keySize];
                        Array.Copy(keyBuffer, result, (int)keySize);
                        return result;
                    }
                }
            }
        }

        public void AssertLiveliness()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_assert_liveliness(_handle));
        }

        public void WaitForAcknowledgments(TimeSpan timeout)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_datawriter_wait_for_acknowledgments(_handle, (long)timeout.TotalMilliseconds));
        }

        public (int totalCount, int currentCount) GetPublicationMatchedStatus()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_get_publication_matched_status(_handle, out var total, out var current));
            return (total, current);
        }

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

        public OfferedDeadlineMissedStatus GetOfferedDeadlineMissedStatus()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            unsafe
            {
                NativeOfferedDeadlineMissedStatus native;
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_datawriter_get_offered_deadline_missed_status(_handle, &native));
                return new OfferedDeadlineMissedStatus(native.TotalCount, native.TotalCountChange);
            }
        }

        public OfferedIncompatibleQosStatus GetOfferedIncompatibleQosStatus()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            unsafe
            {
                NativeOfferedIncompatibleQosStatus native;
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_datawriter_get_offered_incompatible_qos_status(_handle, &native));
                return new OfferedIncompatibleQosStatus(
                    native.TotalCount, native.TotalCountChange, (int)native.LastPolicyId);
            }
        }

        /// <summary>Gets the offered incompatible type status.</summary>
        public OfferedIncompatibleTypeStatus GetOfferedIncompatibleTypeStatus()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            unsafe
            {
                NativeOfferedIncompatibleTypeStatus native;
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_datawriter_get_offered_incompatible_type_status(_handle, &native));
                return new OfferedIncompatibleTypeStatus(native.TotalCount, native.TotalCountChange);
            }
        }

        /// <summary>Gets this DataWriter's 16-byte GUID.</summary>
        public unsafe byte[] GetGuid()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            var guid = new byte[16];
            fixed (byte* p = guid)
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_get_guid(_handle, p));
            }
            return guid;
        }

        /// <summary>
        /// Writes pre-serialized CDR bytes through the zero-copy staging path
        /// (prepare a native buffer, copy into it, then commit). Aborts the loan
        /// on failure.
        /// </summary>
        public unsafe void WriteSerializedStaged(byte[] data, byte[] key = null)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            if (data == null) throw new ArgumentNullException(nameof(data));

            byte* buffer;
            UIntPtr capacity;
            IntPtr loan;
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_prepare_serialized_write(_handle, (UIntPtr)data.Length, out buffer, out capacity, out loan));
            try
            {
                System.Runtime.InteropServices.Marshal.Copy(data, 0, (IntPtr)buffer, data.Length);
                fixed (byte* pKey = key)
                {
                    ReturnCodeHelper.CheckReturn(
                        NativeMethods.int2dds_commit_serialized_write(
                            _handle, loan, (UIntPtr)data.Length,
                            pKey, key != null ? (UIntPtr)key.Length : UIntPtr.Zero));
                }
            }
            catch
            {
                NativeMethods.int2dds_abort_serialized_write(loan);
                throw;
            }
        }

        /// <summary>
        /// Gets the current QoS policies of this DataWriter.
        /// </summary>
        public DataWriterQos GetQos()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_get_qos(_handle, out var qosHandle));
            try
            {
                return ReadWriterQos(qosHandle);
            }
            finally
            {
                NativeMethods.int2dds_datawriter_qos_destroy(qosHandle);
            }
        }

        /// <summary>
        /// Sets new QoS policies on this DataWriter.
        /// </summary>
        public void SetQos(DataWriterQos qos)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_get_qos(_handle, out var qosHandle));
            try
            {
                ApplyWriterQos(qosHandle, qos);
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_set_qos(_handle, qosHandle));
            }
            finally
            {
                NativeMethods.int2dds_datawriter_qos_destroy(qosHandle);
            }
        }

        /// <summary>
        /// Sets or replaces the listener for this DataWriter.
        /// </summary>
        public void SetListener(IDataWriterListener listener, uint statusMask)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

            // Free old listener if any
            if (_listenerContextHandle != IntPtr.Zero)
            {
                ListenerRegistry.FreeListener(_listenerContextHandle);
                _listenerContextHandle = IntPtr.Zero;
            }

            unsafe
            {
                if (listener != null)
                {
                    var (nativeListener, contextHandle) = ListenerRegistry.CreateWriterListener(listener, this);
                    _listenerContextHandle = contextHandle;
                    try
                    {
                        ReturnCodeHelper.CheckReturn(
                            NativeMethods.int2dds_datawriter_set_listener(_handle, &nativeListener, statusMask));
                    }
                    catch
                    {
                        ListenerRegistry.FreeListener(_listenerContextHandle);
                        _listenerContextHandle = IntPtr.Zero;
                        throw;
                    }
                }
                else
                {
                    ReturnCodeHelper.CheckReturn(
                        NativeMethods.int2dds_datawriter_set_listener(_handle, null, 0));
                }
            }
        }

        /// <summary>
        /// Gets the StatusCondition associated with this DataWriter.
        /// </summary>
        public StatusCondition GetStatusCondition()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_datawriter_get_statuscondition(_handle, out var conditionHandle));
            return new StatusCondition(conditionHandle);
        }

        /// <summary>
        /// Gets the current status change bitmask of this DataWriter.
        /// </summary>
        public uint GetStatusChanges()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_datawriter_get_status_changes(_handle, out var mask));
            return mask;
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

        private static DataWriterQos ReadWriterQos(IntPtr h)
        {
            NativeMethods.int2dds_datawriter_qos_get_reliability(h, out var relKind, out var relTime);
            NativeMethods.int2dds_datawriter_qos_get_durability(h, out var durKind);
            NativeMethods.int2dds_datawriter_qos_get_history(h, out var histKind, out var histDepth);
            NativeMethods.int2dds_datawriter_qos_get_ownership(h, out var ownKind);
            NativeMethods.int2dds_datawriter_qos_get_ownership_strength(h, out var ownStr);
            NativeMethods.int2dds_datawriter_qos_get_resource_limits(h, out var maxS, out var maxI, out var maxPI);
            NativeMethods.int2dds_datawriter_qos_get_lifespan(h, out var lifespanNs);
            NativeMethods.int2dds_datawriter_qos_get_destination_order(h, out var destKind);
            NativeMethods.int2dds_datawriter_qos_get_deadline(h, out var deadlineNs);
            NativeMethods.int2dds_datawriter_qos_get_liveliness(h, out var liveKind, out var liveNs);
            NativeMethods.int2dds_datawriter_qos_get_data_representation(h, out var reprKind);
            NativeMethods.int2dds_datawriter_qos_get_transport_priority(h, out var transPri);
            NativeMethods.int2dds_datawriter_qos_get_latency_budget(h, out var latNs);
            NativeMethods.int2dds_datawriter_qos_get_writer_data_lifecycle(h, out var autoDispose);

            return new DataWriterQos
            {
                Reliability = new Reliability((ReliabilityKind)relKind, TimeSpan.FromTicks(relTime / 100)),
                Durability = new Durability((DurabilityKind)durKind),
                History = new History((HistoryKind)histKind, histDepth),
                Ownership = new Ownership((OwnershipKind)ownKind),
                OwnershipStrength = new OwnershipStrength(ownStr),
                ResourceLimits = new ResourceLimits(maxS, maxI, maxPI),
                Lifespan = new Lifespan(TimeSpan.FromTicks(lifespanNs / 100)),
                DestinationOrder = new DestinationOrder((DestinationOrderKind)destKind),
                Deadline = new Deadline(TimeSpan.FromTicks(deadlineNs / 100)),
                Liveliness = new Liveliness((LivelinessKind)liveKind, TimeSpan.FromTicks(liveNs / 100)),
                DataRepresentation = new DataRepresentation((DataRepresentationKind)reprKind),
                TransportPriority = new TransportPriority(transPri),
                LatencyBudget = new LatencyBudget(TimeSpan.FromTicks(latNs / 100)),
                WriterDataLifecycle = new WriterDataLifecycle(autoDispose),
            };
        }

        /// <summary>
        /// Releases all resources used by the DataWriter.
        /// </summary>
        public void Dispose()
        {
            if (_disposed) return;
            _disposed = true;
            GC.SuppressFinalize(this);

            if (_listenerContextHandle != IntPtr.Zero)
            {
                ListenerRegistry.FreeListener(_listenerContextHandle);
                _listenerContextHandle = IntPtr.Zero;
            }

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
