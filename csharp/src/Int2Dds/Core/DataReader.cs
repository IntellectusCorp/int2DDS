using System.Buffers;
using Int2Dds.Conditions;
using Int2Dds.Exceptions;
using Int2Dds.Interop;
using Int2Dds.Listeners;
using Int2Dds.Qos;
using Int2Dds.Types;

namespace Int2Dds.Core;

/// <summary>
/// DataReader - receives data samples from a topic.
///
/// DataReaders are created through Subscriber.CreateDataReader().
/// </summary>
/// <typeparam name="T">The DDS data type.</typeparam>
public sealed class DataReader<T> : IDisposable where T : IDdsType<T>
{
    private const int DefaultBufferSize = 65536;

    private readonly nint _handle;
    private readonly Topic<T> _topic;
    private nint _listenerContextHandle;
    private bool _disposed;

    /// <summary>
    /// Creates a new DataReader. Normally called via Subscriber.CreateDataReader.
    /// </summary>
    internal DataReader(Subscriber subscriber, Topic<T> topic, DataReaderQos? qos = null,
        IDataReaderListener? listener = null, uint statusMask = 0)
    {
        _topic = topic;

        // Create QoS handle if provided
        nint qosHandle = 0;
        if (qos is not null)
        {
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_create_default(out qosHandle));
            try
            {
                ApplyReaderQos(qosHandle, qos);
            }
            catch
            {
                NativeMethods.int2dds_datareader_qos_destroy(qosHandle);
                throw;
            }
        }

        try
        {
            if (listener is not null)
            {
                unsafe
                {
                    var (nativeListener, contextHandle) = ListenerRegistry.CreateReaderListener(listener, this);
                    _listenerContextHandle = contextHandle;
                    try
                    {
                        ReturnCodeHelper.CheckReturn(
                            NativeMethods.int2dds_create_datareader_with_listener(
                                subscriber.Handle, topic.Handle, qosHandle, &nativeListener, statusMask, out _handle));
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
                    NativeMethods.int2dds_create_datareader(subscriber.Handle, topic.Handle, qosHandle, out _handle));
            }
        }
        finally
        {
            if (qosHandle != 0)
                NativeMethods.int2dds_datareader_qos_destroy(qosHandle);
        }
    }

    /// <summary>
    /// Gets the native handle. For internal use.
    /// </summary>
    internal nint Handle => _handle;

    /// <summary>
    /// Gets the topic this reader subscribes to.
    /// </summary>
    public Topic<T> Topic => _topic;

    /// <summary>
    /// Gets the current number of matched writers.
    /// </summary>
    public int MatchedWriters
    {
        get
        {
            var (_, currentCount) = GetSubscriptionMatchedStatus();
            return currentCount;
        }
    }

    /// <summary>
    /// Take all available samples, removing them from the reader cache.
    /// </summary>
    /// <returns>A list of samples.</returns>
    public IReadOnlyList<Sample<T>> Take()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);

        var samples = new List<Sample<T>>();
        while (true)
        {
            var sample = TakeOneSample();
            if (sample is null)
                break;
            samples.Add(sample.Value);
        }
        return samples;
    }

    /// <summary>
    /// Read all available samples without removing them from the reader cache.
    /// </summary>
    /// <returns>A list of samples.</returns>
    public IReadOnlyList<Sample<T>> Read()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);

        var samples = new List<Sample<T>>();
        var sample = ReadOneSample();
        if (sample is not null)
            samples.Add(sample.Value);
        return samples;
    }

    /// <summary>
    /// Take a single sample, removing it from the reader cache.
    /// </summary>
    /// <returns>A sample, or null if no data is available.</returns>
    public Sample<T>? TakeOne()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
        return TakeOneSample();
    }

    /// <summary>
    /// Read a single sample without removing it from the reader cache.
    /// </summary>
    /// <returns>A sample, or null if no data is available.</returns>
    public Sample<T>? ReadOne()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
        return ReadOneSample();
    }

    /// <summary>
    /// Take all available samples with their associated SampleInfo metadata.
    /// </summary>
    /// <returns>A list of (sample, info) tuples.</returns>
    public IReadOnlyList<(Sample<T> Sample, SampleInfo Info)> TakeWithInfo()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);

        var results = new List<(Sample<T>, SampleInfo)>();
        while (true)
        {
            var result = TakeOneSampleWithInfo();
            if (result is null)
                break;
            results.Add(result.Value);
        }
        return results;
    }

    /// <summary>
    /// Read all available samples with their associated SampleInfo metadata.
    /// </summary>
    /// <returns>A list of (sample, info) tuples.</returns>
    public IReadOnlyList<(Sample<T> Sample, SampleInfo Info)> ReadWithInfo()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);

        var results = new List<(Sample<T>, SampleInfo)>();
        var result = ReadOneSampleWithInfo();
        if (result is not null)
            results.Add(result.Value);
        return results;
    }

    /// <summary>
    /// Take a batch of samples with their associated SampleInfo metadata.
    /// Uses the batch FFI for efficient multi-sample retrieval.
    /// </summary>
    /// <param name="maxSamples">Maximum number of samples to take.</param>
    /// <returns>A list of (sample, info) tuples.</returns>
    public IReadOnlyList<(Sample<T> Sample, SampleInfo Info)> TakeBatch(int maxSamples)
    {
        ObjectDisposedException.ThrowIf(_disposed, this);

        var ret = NativeMethods.int2dds_take_serialized_batch(_handle, maxSamples, out var seqHandle);
        if (ret == ReturnCode.NoData)
            return Array.Empty<(Sample<T>, SampleInfo)>();
        ReturnCodeHelper.CheckReturn(ret);

        try
        {
            return ReadSampleSequence(seqHandle);
        }
        finally
        {
            NativeMethods.int2dds_sample_seq_delete(seqHandle);
        }
    }

    /// <summary>
    /// Read a batch of samples with their associated SampleInfo metadata.
    /// Uses the batch FFI for efficient multi-sample retrieval.
    /// </summary>
    /// <param name="maxSamples">Maximum number of samples to read.</param>
    /// <returns>A list of (sample, info) tuples.</returns>
    public IReadOnlyList<(Sample<T> Sample, SampleInfo Info)> ReadBatch(int maxSamples)
    {
        ObjectDisposedException.ThrowIf(_disposed, this);

        var ret = NativeMethods.int2dds_read_serialized_batch(_handle, maxSamples, out var seqHandle);
        if (ret == ReturnCode.NoData)
            return Array.Empty<(Sample<T>, SampleInfo)>();
        ReturnCodeHelper.CheckReturn(ret);

        try
        {
            return ReadSampleSequence(seqHandle);
        }
        finally
        {
            NativeMethods.int2dds_sample_seq_delete(seqHandle);
        }
    }

    /// <summary>
    /// Block until historical data is available (for TRANSIENT_LOCAL or TRANSIENT durability).
    /// </summary>
    /// <param name="timeout">Maximum time to wait.</param>
    public void WaitForHistoricalData(TimeSpan timeout)
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
        ReturnCodeHelper.CheckReturn(
            NativeMethods.int2dds_datareader_wait_for_historical_data(_handle, (long)timeout.TotalMilliseconds));
    }

    /// <summary>
    /// Gets the subscription matched status.
    /// </summary>
    /// <returns>A tuple of (totalCount, currentCount) indicating matched writers.</returns>
    public (int totalCount, int currentCount) GetSubscriptionMatchedStatus()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
        ReturnCodeHelper.CheckReturn(
            NativeMethods.int2dds_get_subscription_matched_status(_handle, out var total, out var current));
        return (total, current);
    }

    /// <summary>
    /// Gets the liveliness changed status.
    /// </summary>
    public LivelinessChangedStatus GetLivelinessChangedStatus()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);

        unsafe
        {
            NativeLivelinessChangedStatus native;
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_datareader_get_liveliness_changed_status(_handle, &native));

            return new LivelinessChangedStatus(
                native.AliveCount,
                native.NotAliveCount,
                native.AliveCountChange,
                native.NotAliveCountChange);
        }
    }

    /// <summary>
    /// Gets the sample rejected status.
    /// </summary>
    public SampleRejectedStatus GetSampleRejectedStatus()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);

        unsafe
        {
            NativeSampleRejectedStatus native;
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_datareader_get_sample_rejected_status(_handle, &native));

            return new SampleRejectedStatus(
                native.TotalCount,
                native.TotalCountChange,
                (int)native.LastReason);
        }
    }

    /// <summary>
    /// Gets the sample lost status.
    /// </summary>
    public SampleLostStatus GetSampleLostStatus()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);

        unsafe
        {
            NativeSampleLostStatus native;
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_datareader_get_sample_lost_status(_handle, &native));
            return new SampleLostStatus(native.TotalCount, native.TotalCountChange);
        }
    }

    /// <summary>
    /// Gets the requested deadline missed status.
    /// </summary>
    public RequestedDeadlineMissedStatus GetRequestedDeadlineMissedStatus()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);

        unsafe
        {
            NativeRequestedDeadlineMissedStatus native;
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_datareader_get_requested_deadline_missed_status(_handle, &native));

            return new RequestedDeadlineMissedStatus(
                native.TotalCount,
                native.TotalCountChange);
        }
    }

    /// <summary>
    /// Gets the requested incompatible QoS status.
    /// </summary>
    public RequestedIncompatibleQosStatus GetRequestedIncompatibleQosStatus()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);

        unsafe
        {
            NativeRequestedIncompatibleQosStatus native;
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_datareader_get_requested_incompatible_qos_status(_handle, &native));
            return new RequestedIncompatibleQosStatus(
                native.TotalCount,
                native.TotalCountChange,
                (int)native.LastPolicyId);
        }
    }

    /// <summary>
    /// Sets new QoS policies on this DataReader.
    /// Some policies can only be changed before the entity is enabled.
    /// </summary>
    /// <param name="qos">The new QoS policies to apply.</param>
    /// <summary>
    /// Gets the current QoS policies of this DataReader.
    /// </summary>
    public DataReaderQos GetQos()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);

        ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_get_qos(_handle, out var qosHandle));
        try
        {
            return ReadReaderQos(qosHandle);
        }
        finally
        {
            NativeMethods.int2dds_datareader_qos_destroy(qosHandle);
        }
    }

    public void SetQos(DataReaderQos qos)
    {
        ObjectDisposedException.ThrowIf(_disposed, this);

        // Get current QoS as base, then apply user overrides on top
        ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_get_qos(_handle, out var qosHandle));
        try
        {
            ApplyReaderQos(qosHandle, qos);
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_set_qos(_handle, qosHandle));
        }
        finally
        {
            NativeMethods.int2dds_datareader_qos_destroy(qosHandle);
        }
    }

    /// <summary>
    /// Sets or replaces the listener for this DataReader.
    /// </summary>
    /// <param name="listener">The listener to set, or null to remove.</param>
    /// <param name="statusMask">Bitmask of statuses to listen for.</param>
    public void SetListener(IDataReaderListener? listener, uint statusMask)
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
                var (nativeListener, contextHandle) = ListenerRegistry.CreateReaderListener(listener, this);
                _listenerContextHandle = contextHandle;
                try
                {
                    ReturnCodeHelper.CheckReturn(
                        NativeMethods.int2dds_datareader_set_listener(_handle, &nativeListener, statusMask));
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
                    NativeMethods.int2dds_datareader_set_listener(_handle, null, 0));
            }
        }
    }

    /// <summary>
    /// Gets the StatusCondition associated with this DataReader.
    /// </summary>
    /// <returns>A StatusCondition for use with WaitSets.</returns>
    public StatusCondition GetStatusCondition()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
        ReturnCodeHelper.CheckReturn(
            NativeMethods.int2dds_datareader_get_statuscondition(_handle, out var conditionHandle));
        return new StatusCondition(conditionHandle);
    }

    // ── Private helpers ──────────────────────────────────────────────────

    /// <summary>
    /// Takes one sample using the batch API to avoid data loss on buffer overflow.
    /// The batch API manages memory on the Rust side, so buffer size is never an issue.
    /// </summary>
    private Sample<T>? TakeOneSample()
    {
        var ret = NativeMethods.int2dds_take_serialized_batch(_handle, 1, out var seqHandle);
        if (ret == ReturnCode.NoData)
            return null;
        ReturnCodeHelper.CheckReturn(ret);

        try
        {
            var length = (int)NativeMethods.int2dds_sample_seq_length(seqHandle);
            if (length == 0)
                return null;

            unsafe
            {
                NativeSampleInfo nativeInfo;
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_sample_seq_get_info(seqHandle, 0, &nativeInfo));

                if (nativeInfo.ValidData)
                {
                    var buffer = ArrayPool<byte>.Shared.Rent(DefaultBufferSize);
                    try
                    {
                        var data = ReadSampleData(seqHandle, 0, buffer);
                        return new Sample<T>(data, true);
                    }
                    finally
                    {
                        ArrayPool<byte>.Shared.Return(buffer);
                    }
                }
                return new Sample<T>(default, false);
            }
        }
        finally
        {
            NativeMethods.int2dds_sample_seq_delete(seqHandle);
        }
    }

    private Sample<T>? ReadOneSample()
    {
        var buffer = ArrayPool<byte>.Shared.Rent(DefaultBufferSize);
        try
        {
            unsafe
            {
                fixed (byte* pBuffer = buffer)
                {
                    var ret = NativeMethods.int2dds_read_serialized(
                        _handle, pBuffer, (nuint)buffer.Length, out var actualSize, out var validData);

                    if (ret == ReturnCode.NoData)
                        return null;
                    ReturnCodeHelper.CheckReturn(ret);

                    if (validData)
                    {
                        var data = T.DeserializeCdr(new ReadOnlySpan<byte>(buffer, 0, (int)actualSize));
                        return new Sample<T>(data, true);
                    }
                    return new Sample<T>(default, false);
                }
            }
        }
        finally
        {
            ArrayPool<byte>.Shared.Return(buffer);
        }
    }

    /// <summary>
    /// Takes one sample with info using the batch API to avoid data loss on buffer overflow.
    /// </summary>
    private (Sample<T> Sample, SampleInfo Info)? TakeOneSampleWithInfo()
    {
        var ret = NativeMethods.int2dds_take_serialized_batch(_handle, 1, out var seqHandle);
        if (ret == ReturnCode.NoData)
            return null;
        ReturnCodeHelper.CheckReturn(ret);

        try
        {
            var length = (int)NativeMethods.int2dds_sample_seq_length(seqHandle);
            if (length == 0)
                return null;

            unsafe
            {
                NativeSampleInfo nativeInfo;
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_sample_seq_get_info(seqHandle, 0, &nativeInfo));

                var info = ConvertSampleInfo(ref nativeInfo);

                if (nativeInfo.ValidData)
                {
                    var buffer = ArrayPool<byte>.Shared.Rent(DefaultBufferSize);
                    try
                    {
                        var data = ReadSampleData(seqHandle, 0, buffer);
                        return (new Sample<T>(data, true), info);
                    }
                    finally
                    {
                        ArrayPool<byte>.Shared.Return(buffer);
                    }
                }
                return (new Sample<T>(default, false), info);
            }
        }
        finally
        {
            NativeMethods.int2dds_sample_seq_delete(seqHandle);
        }
    }

    private (Sample<T> Sample, SampleInfo Info)? ReadOneSampleWithInfo()
    {
        var buffer = ArrayPool<byte>.Shared.Rent(DefaultBufferSize);
        try
        {
            unsafe
            {
                fixed (byte* pBuffer = buffer)
                {
                    NativeSampleInfo nativeInfo;
                    var ret = NativeMethods.int2dds_read_serialized_w_info(
                        _handle, pBuffer, (nuint)buffer.Length, out var actualSize, &nativeInfo);

                    if (ret == ReturnCode.NoData)
                        return null;
                    ReturnCodeHelper.CheckReturn(ret);

                    var info = ConvertSampleInfo(ref nativeInfo);
                    if (nativeInfo.ValidData)
                    {
                        var data = T.DeserializeCdr(new ReadOnlySpan<byte>(buffer, 0, (int)actualSize));
                        return (new Sample<T>(data, true), info);
                    }
                    return (new Sample<T>(default, false), info);
                }
            }
        }
        finally
        {
            ArrayPool<byte>.Shared.Return(buffer);
        }
    }

    private IReadOnlyList<(Sample<T> Sample, SampleInfo Info)> ReadSampleSequence(nint seqHandle)
    {
        var length = (int)NativeMethods.int2dds_sample_seq_length(seqHandle);
        var results = new List<(Sample<T>, SampleInfo)>(length);

        var buffer = ArrayPool<byte>.Shared.Rent(DefaultBufferSize);
        try
        {
            unsafe
            {
                for (nuint i = 0; i < (nuint)length; i++)
                {
                    NativeSampleInfo nativeInfo;
                    ReturnCodeHelper.CheckReturn(
                        NativeMethods.int2dds_sample_seq_get_info(seqHandle, i, &nativeInfo));

                    var info = ConvertSampleInfo(ref nativeInfo);

                    if (nativeInfo.ValidData)
                    {
                        var data = ReadSampleData(seqHandle, i, buffer);
                        results.Add((new Sample<T>(data, true), info));
                    }
                    else
                    {
                        results.Add((new Sample<T>(default, false), info));
                    }
                }
            }
        }
        finally
        {
            ArrayPool<byte>.Shared.Return(buffer);
        }

        return results;
    }

    /// <summary>
    /// Reads sample data from a sequence handle, retrying with a larger buffer if needed.
    /// </summary>
    private static unsafe T ReadSampleData(nint seqHandle, nuint index, byte[] buffer)
    {
        fixed (byte* pBuffer = buffer)
        {
            var ret = NativeMethods.int2dds_sample_seq_get_data(seqHandle, index, pBuffer,
                (nuint)buffer.Length, out var actualSize);

            if (ret == ReturnCode.Ok)
            {
                return T.DeserializeCdr(new ReadOnlySpan<byte>(buffer, 0, (int)actualSize));
            }

            // Buffer too small — allocate exact size and retry
            if ((int)actualSize > buffer.Length)
            {
                var largeBuffer = new byte[(int)actualSize];
                fixed (byte* pLarge = largeBuffer)
                {
                    ReturnCodeHelper.CheckReturn(
                        NativeMethods.int2dds_sample_seq_get_data(seqHandle, index, pLarge,
                            (nuint)largeBuffer.Length, out actualSize));
                    return T.DeserializeCdr(new ReadOnlySpan<byte>(largeBuffer, 0, (int)actualSize));
                }
            }

            ReturnCodeHelper.CheckReturn(ret);
            return default!; // unreachable
        }
    }

    private static SampleInfo ConvertSampleInfo(ref NativeSampleInfo native)
    {
        Span<byte> instanceHandleBytes = stackalloc byte[16];
        Span<byte> pubHandleBytes = stackalloc byte[16];

        System.Runtime.InteropServices.MemoryMarshal.CreateReadOnlySpan(
            ref System.Runtime.CompilerServices.Unsafe.As<Handle16, byte>(ref native.InstanceHandle), 16)
            .CopyTo(instanceHandleBytes);

        System.Runtime.InteropServices.MemoryMarshal.CreateReadOnlySpan(
            ref System.Runtime.CompilerServices.Unsafe.As<Handle16, byte>(ref native.PublicationHandle), 16)
            .CopyTo(pubHandleBytes);

        return new SampleInfo
        {
            SourceTimestampSec = native.SourceTimestampSec,
            SourceTimestampNanosec = native.SourceTimestampNanosec,
            SampleState = native.SampleState,
            ViewState = native.ViewState,
            InstanceState = native.InstanceState,
            InstanceHandle = new InstanceHandle(instanceHandleBytes),
            PublicationHandle = new InstanceHandle(pubHandleBytes),
            DisposedGenerationCount = native.DisposedGenerationCount,
            NoWritersGenerationCount = native.NoWritersGenerationCount,
            SampleRank = native.SampleRank,
            GenerationRank = native.GenerationRank,
            AbsoluteGenerationRank = native.AbsoluteGenerationRank,
            ValidData = native.ValidData,
        };
    }

    private static void ApplyReaderQos(nint qosHandle, DataReaderQos qos)
    {
        if (qos.Reliability is not null)
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_reliability(
                qosHandle, (int)qos.Reliability.Kind, qos.Reliability.MaxBlockingTimeNs));

        if (qos.Durability is not null)
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_durability(
                qosHandle, (int)qos.Durability.Kind));

        if (qos.History is not null)
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_history(
                qosHandle, (int)qos.History.Kind, qos.History.Depth));

        if (qos.Ownership is not null)
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_ownership(
                qosHandle, (int)qos.Ownership.Kind));

        if (qos.ResourceLimits is not null)
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_resource_limits(
                qosHandle, qos.ResourceLimits.MaxSamples, qos.ResourceLimits.MaxInstances,
                qos.ResourceLimits.MaxSamplesPerInstance));

        if (qos.DestinationOrder is not null)
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_destination_order(
                qosHandle, (int)qos.DestinationOrder.Kind));

        if (qos.TimeBasedFilter is not null)
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_time_based_filter(
                qosHandle, qos.TimeBasedFilter.MinimumSeparationNs));

        if (qos.LatencyBudget is not null)
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_latency_budget(
                qosHandle, qos.LatencyBudget.DurationNs));

        if (qos.UserData is { Data.Length: > 0 })
        {
            unsafe
            {
                fixed (byte* pData = qos.UserData.Data)
                {
                    ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_user_data(
                        qosHandle, pData, (nuint)qos.UserData.Data.Length));
                }
            }
        }

        if (qos.ReaderDataLifecycle is not null)
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_reader_data_lifecycle(
                qosHandle, qos.ReaderDataLifecycle.AutopurgeNowriterNs,
                qos.ReaderDataLifecycle.AutopurgeDisposedNs));

        if (qos.DataRepresentation is not null)
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_data_representation(
                qosHandle, (int)qos.DataRepresentation.Kind));

        if (qos.Deadline is not null)
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_deadline(
                qosHandle, qos.Deadline.PeriodNs));

        if (qos.Liveliness is not null)
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_liveliness(
                qosHandle, (int)qos.Liveliness.Kind, qos.Liveliness.LeaseDurationNs));
    }

    private static DataReaderQos ReadReaderQos(nint h)
    {
        NativeMethods.int2dds_datareader_qos_get_reliability(h, out var relKind, out var relTime);
        NativeMethods.int2dds_datareader_qos_get_durability(h, out var durKind);
        NativeMethods.int2dds_datareader_qos_get_history(h, out var histKind, out var histDepth);
        NativeMethods.int2dds_datareader_qos_get_ownership(h, out var ownKind);
        NativeMethods.int2dds_datareader_qos_get_resource_limits(h, out var maxS, out var maxI, out var maxPI);
        NativeMethods.int2dds_datareader_qos_get_destination_order(h, out var destKind);
        NativeMethods.int2dds_datareader_qos_get_deadline(h, out var deadlineNs);
        NativeMethods.int2dds_datareader_qos_get_liveliness(h, out var liveKind, out var liveNs);
        NativeMethods.int2dds_datareader_qos_get_data_representation(h, out var reprKind);
        NativeMethods.int2dds_datareader_qos_get_latency_budget(h, out var latNs);
        NativeMethods.int2dds_datareader_qos_get_time_based_filter(h, out var tbfNs);
        NativeMethods.int2dds_datareader_qos_get_reader_data_lifecycle(h, out var purgeNowriterNs, out var purgeDisposedNs);

        return new DataReaderQos
        {
            Reliability = new Reliability((ReliabilityKind)relKind, TimeSpan.FromTicks(relTime / 100)),
            Durability = new Durability((DurabilityKind)durKind),
            History = new History((HistoryKind)histKind, histDepth),
            Ownership = new Ownership((OwnershipKind)ownKind),
            ResourceLimits = new ResourceLimits(maxS, maxI, maxPI),
            DestinationOrder = new DestinationOrder((DestinationOrderKind)destKind),
            Deadline = new Deadline(TimeSpan.FromTicks(deadlineNs / 100)),
            Liveliness = new Liveliness((LivelinessKind)liveKind, TimeSpan.FromTicks(liveNs / 100)),
            DataRepresentation = new DataRepresentation((DataRepresentationKind)reprKind),
            LatencyBudget = new LatencyBudget(TimeSpan.FromTicks(latNs / 100)),
            TimeBasedFilter = new TimeBasedFilter(TimeSpan.FromTicks(tbfNs / 100)),
            ReaderDataLifecycle = new ReaderDataLifecycle(
                TimeSpan.FromTicks(purgeNowriterNs / 100),
                TimeSpan.FromTicks(purgeDisposedNs / 100)),
        };
    }

    /// <summary>
    /// Releases all resources used by the DataReader.
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

        NativeMethods.int2dds_delete_datareader(_handle);
    }

    ~DataReader()
    {
        if (!_disposed)
        {
            try { Dispose(); }
            catch { /* suppress errors during finalization */ }
        }
    }
}
