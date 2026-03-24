using Int2Dds.Conditions;
using Int2Dds.Exceptions;
using Int2Dds.Interop;
using Int2Dds.Listeners;
using Int2Dds.Qos;
using Int2Dds.Types;

namespace Int2Dds.Core;

/// <summary>
/// DataWriter - publishes data samples to a topic.
///
/// DataWriters are created through Publisher.CreateDataWriter().
/// </summary>
/// <typeparam name="T">The DDS data type.</typeparam>
public sealed class DataWriter<T> : IDisposable where T : IDdsType<T>
{
    private readonly nint _handle;
    private readonly Topic<T> _topic;
    private nint _listenerContextHandle;
    private bool _disposed;

    /// <summary>
    /// Creates a new DataWriter. Normally called via Publisher.CreateDataWriter.
    /// </summary>
    internal DataWriter(Publisher publisher, Topic<T> topic, DataWriterQos? qos = null,
        IDataWriterListener? listener = null, uint statusMask = 0)
    {
        _topic = topic;

        // Create QoS handle if provided
        nint qosHandle = 0;
        if (qos is not null)
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
            if (listener is not null)
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
                        _listenerContextHandle = 0;
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
            if (qosHandle != 0)
                NativeMethods.int2dds_datawriter_qos_destroy(qosHandle);
        }
    }

    /// <summary>
    /// Gets the native handle. For internal use.
    /// </summary>
    internal nint Handle => _handle;

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
        ObjectDisposedException.ThrowIf(_disposed, this);

        var data = sample.SerializeCdr();
        var key = T.HasKey ? sample.SerializeKey() : null;

        unsafe
        {
            fixed (byte* pData = data)
            fixed (byte* pKey = key)
            {
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_write_serialized(
                        _handle,
                        pData, (nuint)data.Length,
                        pKey, key is not null ? (nuint)key.Length : 0));
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
        ObjectDisposedException.ThrowIf(_disposed, this);

        var data = sample.SerializeCdr();
        var key = T.HasKey ? sample.SerializeKey() : null;

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
                        pData, (nuint)data.Length,
                        pKey, key is not null ? (nuint)key.Length : 0,
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
        ObjectDisposedException.ThrowIf(_disposed, this);

        var key = T.HasKey ? sample.SerializeKey() : null;
        if (key is null || key.Length == 0)
            return InstanceHandle.Nil;

        var handleBytes = new byte[16];

        unsafe
        {
            fixed (byte* pKey = key)
            fixed (byte* pHandle = handleBytes)
            {
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_datawriter_register_instance(
                        _handle, pKey, (nuint)key.Length, pHandle));
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
        ObjectDisposedException.ThrowIf(_disposed, this);

        var key = T.HasKey ? sample.SerializeKey() : null;
        if (key is null || key.Length == 0)
            return;

        var handleBytes = handle.ToByteArray();

        unsafe
        {
            fixed (byte* pKey = key)
            fixed (byte* pHandle = handleBytes)
            {
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_datawriter_unregister_instance(
                        _handle, pKey, (nuint)key.Length, pHandle));
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
        ObjectDisposedException.ThrowIf(_disposed, this);

        var key = T.HasKey ? sample.SerializeKey() : null;
        if (key is null || key.Length == 0)
            return;

        var handleBytes = handle.ToByteArray();

        unsafe
        {
            fixed (byte* pKey = key)
            fixed (byte* pHandle = handleBytes)
            {
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_datawriter_dispose(
                        _handle, pKey, (nuint)key.Length, pHandle));
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
        ObjectDisposedException.ThrowIf(_disposed, this);

        var key = T.HasKey ? sample.SerializeKey() : null;
        if (key is null || key.Length == 0)
            return InstanceHandle.Nil;

        var handleBytes = new byte[16];

        unsafe
        {
            fixed (byte* pKey = key)
            fixed (byte* pHandle = handleBytes)
            {
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_datawriter_lookup_instance(
                        _handle, pKey, (nuint)key.Length, pHandle));
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
        ObjectDisposedException.ThrowIf(_disposed, this);

        var handleBytes = handle.ToByteArray();
        var keyBuffer = new byte[256];

        unsafe
        {
            fixed (byte* pHandle = handleBytes)
            {
                nuint keySize;
                fixed (byte* pKey = keyBuffer)
                {
                    var ret = NativeMethods.int2dds_datawriter_get_key_value(
                        _handle, pHandle, pKey, (nuint)keyBuffer.Length, out keySize);

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
                            _handle, pHandle, pKey, (nuint)keyBuffer.Length, out keySize));

                    var result = new byte[(int)keySize];
                    Array.Copy(keyBuffer, result, (int)keySize);
                    return result;
                }
            }
        }
    }

    /// <summary>
    /// Assert liveliness for MANUAL_BY_TOPIC liveliness.
    /// </summary>
    public void AssertLiveliness()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
        ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_assert_liveliness(_handle));
    }

    /// <summary>
    /// Waits until all written data has been acknowledged by matched readers.
    /// </summary>
    /// <param name="timeout">Maximum time to wait.</param>
    public void WaitForAcknowledgments(TimeSpan timeout)
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
        ReturnCodeHelper.CheckReturn(
            NativeMethods.int2dds_datawriter_wait_for_acknowledgments(_handle, (long)timeout.TotalMilliseconds));
    }

    /// <summary>
    /// Gets the publication matched status.
    /// </summary>
    /// <returns>A tuple of (totalCount, currentCount) indicating matched readers.</returns>
    public (int totalCount, int currentCount) GetPublicationMatchedStatus()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
        ReturnCodeHelper.CheckReturn(
            NativeMethods.int2dds_get_publication_matched_status(_handle, out var total, out var current));
        return (total, current);
    }

    /// <summary>
    /// Gets the liveliness lost status.
    /// </summary>
    public LivelinessLostStatus GetLivelinessLostStatus()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);

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
        ObjectDisposedException.ThrowIf(_disposed, this);

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
        ObjectDisposedException.ThrowIf(_disposed, this);

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
    /// Sets new QoS policies on this DataWriter.
    /// Some policies can only be changed before the entity is enabled.
    /// </summary>
    /// <param name="qos">The new QoS policies to apply.</param>
    /// <summary>
    /// Gets the current QoS policies of this DataWriter.
    /// </summary>
    public DataWriterQos GetQos()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);

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

    public void SetQos(DataWriterQos qos)
    {
        ObjectDisposedException.ThrowIf(_disposed, this);

        // Get current QoS as base, then apply user overrides on top
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
    /// <param name="listener">The listener to set, or null to remove.</param>
    /// <param name="statusMask">Bitmask of statuses to listen for.</param>
    public void SetListener(IDataWriterListener? listener, uint statusMask)
    {
        ObjectDisposedException.ThrowIf(_disposed, this);

        // Free old listener if any
        if (_listenerContextHandle != 0)
        {
            ListenerRegistry.FreeListener(_listenerContextHandle);
            _listenerContextHandle = 0;
        }

        unsafe
        {
            if (listener is not null)
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
                    _listenerContextHandle = 0;
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
    /// <returns>A StatusCondition for use with WaitSets.</returns>
    public StatusCondition GetStatusCondition()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
        ReturnCodeHelper.CheckReturn(
            NativeMethods.int2dds_datawriter_get_statuscondition(_handle, out var conditionHandle));
        return new StatusCondition(conditionHandle);
    }

    private static void ApplyWriterQos(nint qosHandle, DataWriterQos qos)
    {
        if (qos.Reliability is not null)
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_reliability(
                qosHandle, (int)qos.Reliability.Kind, qos.Reliability.MaxBlockingTimeNs));

        if (qos.Durability is not null)
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_durability(
                qosHandle, (int)qos.Durability.Kind));

        if (qos.History is not null)
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_history(
                qosHandle, (int)qos.History.Kind, qos.History.Depth));

        if (qos.Ownership is not null)
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_ownership(
                qosHandle, (int)qos.Ownership.Kind));

        if (qos.OwnershipStrength is not null)
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_ownership_strength(
                qosHandle, qos.OwnershipStrength.Value));

        if (qos.ResourceLimits is not null)
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_resource_limits(
                qosHandle, qos.ResourceLimits.MaxSamples, qos.ResourceLimits.MaxInstances,
                qos.ResourceLimits.MaxSamplesPerInstance));

        if (qos.Lifespan is not null)
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_lifespan(
                qosHandle, qos.Lifespan.DurationNs));

        if (qos.DestinationOrder is not null)
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_destination_order(
                qosHandle, (int)qos.DestinationOrder.Kind));

        if (qos.LatencyBudget is not null)
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_latency_budget(
                qosHandle, qos.LatencyBudget.DurationNs));

        if (qos.TransportPriority is not null)
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_transport_priority(
                qosHandle, qos.TransportPriority.Value));

        if (qos.UserData is { Data.Length: > 0 })
        {
            unsafe
            {
                fixed (byte* pData = qos.UserData.Data)
                {
                    ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_user_data(
                        qosHandle, pData, (nuint)qos.UserData.Data.Length));
                }
            }
        }

        if (qos.WriterDataLifecycle is not null)
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_writer_data_lifecycle(
                qosHandle, qos.WriterDataLifecycle.AutodisposeUnregisteredInstances));

        if (qos.DataRepresentation is not null)
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_data_representation(
                qosHandle, (int)qos.DataRepresentation.Kind));

        if (qos.Deadline is not null)
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_deadline(
                qosHandle, qos.Deadline.PeriodNs));

        if (qos.Liveliness is not null)
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_liveliness(
                qosHandle, (int)qos.Liveliness.Kind, qos.Liveliness.LeaseDurationNs));
    }

    private static DataWriterQos ReadWriterQos(nint h)
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

        if (_listenerContextHandle != 0)
        {
            ListenerRegistry.FreeListener(_listenerContextHandle);
            _listenerContextHandle = 0;
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
