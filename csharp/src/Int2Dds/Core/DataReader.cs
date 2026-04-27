using System;
#if !NET45
using System.Buffers;
#endif
using System.Collections.Generic;
using System.Reflection;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
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
    /// DataReader - receives data samples from a topic.
    ///
    /// DataReaders are created through Subscriber.CreateDataReader().
    /// </summary>
    /// <typeparam name="T">The DDS data type.</typeparam>
    public sealed class DataReader<T> : IDisposable where T : class, IDdsType, new()
    {
        private const int DefaultBufferSize = 65536;

        private static readonly Func<byte[], T> s_deserializer = CreateDeserializer();

        private static Func<byte[], T> CreateDeserializer()
        {
            var method = typeof(T).GetMethod("DeserializeCdr", BindingFlags.Public | BindingFlags.Static,
                null, new[] { typeof(byte[]) }, null);
            if (method == null)
                throw new InvalidOperationException(
                    $"{typeof(T).Name} must have a public static DeserializeCdr(byte[]) method.");
            return (byte[] data) => (T)method.Invoke(null, new object[] { data });
        }

        private readonly IntPtr _handle;
        private readonly Topic<T> _topic;
        private readonly byte[] _buffer;
        private IntPtr _listenerContextHandle;
        private bool _disposed;

        /// <summary>
        /// Creates a new DataReader. Normally called via Subscriber.CreateDataReader.
        /// </summary>
        internal DataReader(Subscriber subscriber, Topic<T> topic, DataReaderQos? qos = null,
            IDataReaderListener? listener = null, uint statusMask = 0)
        {
            _topic = topic;
            _buffer = RentBuffer(DefaultBufferSize);

            // Always create a QoS handle so that the native layer receives the
            // correct DataRepresentation default (XCDR1) even when the caller
            // does not supply an explicit QoS object.
            IntPtr qosHandle = IntPtr.Zero;
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_create_default(out qosHandle));
            try
            {
                if (qos != null)
                    ApplyReaderQos(qosHandle, qos);

                // Ensure SEDP advertises the same encoding that C# actually uses.
                if (qos?.DataRepresentation == null)
                    ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_data_representation(
                        qosHandle, (int)Qos.DataRepresentationKind.Xcdr1));
            }
            catch
            {
                NativeMethods.int2dds_datareader_qos_destroy(qosHandle);
                throw;
            }

            try
            {
                if (listener != null)
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
                            _listenerContextHandle = IntPtr.Zero;
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
                NativeMethods.int2dds_datareader_qos_destroy(qosHandle);
            }
        }

        /// <summary>
        /// Creates a new DataReader using a QoS profile path.
        /// Normally called via Subscriber.CreateDataReaderWithProfile.
        /// </summary>
        internal DataReader(Subscriber subscriber, Topic<T> topic, string qosPath,
            IDataReaderListener? listener = null, uint statusMask = 0)
        {
            _topic = topic;
            _buffer = RentBuffer(DefaultBufferSize);

            unsafe
            {
                var qosPathBytes = Encoding.UTF8.GetBytes(qosPath + '\0');
                fixed (byte* pQos = qosPathBytes)
                {
                    if (listener != null)
                    {
                        var (nativeListener, contextHandle) = ListenerRegistry.CreateReaderListener(listener, this);
                        _listenerContextHandle = contextHandle;
                        try
                        {
                            ReturnCodeHelper.CheckReturn(
                                NativeMethods.int2dds_create_datareader_with_profile_and_listener(
                                    subscriber.Handle, topic.Handle, pQos, &nativeListener, statusMask, out _handle));
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
                            NativeMethods.int2dds_create_datareader_with_profile(
                                subscriber.Handle, topic.Handle, pQos, out _handle));
                    }
                }
            }
        }

        /// <summary>
        /// Gets the native handle. For internal use.
        /// </summary>
        internal IntPtr Handle => _handle;

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
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

            var samples = new List<Sample<T>>();
            while (true)
            {
                var sample = TakeOneSample();
                if (!sample.HasValue)
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
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

            var samples = new List<Sample<T>>();
            var sample = ReadOneSample();
            if (sample.HasValue)
                samples.Add(sample.Value);
            return samples;
        }

        /// <summary>
        /// Take a single sample, removing it from the reader cache.
        /// </summary>
        /// <returns>A sample, or null if no data is available.</returns>
        public Sample<T>? TakeOne()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            return TakeOneSample();
        }

        /// <summary>
        /// Read a single sample without removing it from the reader cache.
        /// </summary>
        /// <returns>A sample, or null if no data is available.</returns>
        public Sample<T>? ReadOne()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            return ReadOneSample();
        }

        /// <summary>
        /// Take all available samples with their associated SampleInfo metadata.
        /// </summary>
        /// <returns>A list of (sample, info) tuples.</returns>
        public IReadOnlyList<(Sample<T> Sample, SampleInfo Info)> TakeWithInfo()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

            var results = new List<(Sample<T>, SampleInfo)>();
            while (true)
            {
                var result = TakeOneSampleWithInfo();
                if (!result.HasValue)
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
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

            var results = new List<(Sample<T>, SampleInfo)>();
            var result = ReadOneSampleWithInfo();
            if (result.HasValue)
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
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

            var ret = NativeMethods.int2dds_take_serialized_batch(_handle, maxSamples, out var seqHandle);
            if (ret == ReturnCode.NoData)
                return Int2Dds.Internal.EmptyArrayHolder<(Sample<T>, SampleInfo)>.Value;
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
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

            var ret = NativeMethods.int2dds_read_serialized_batch(_handle, maxSamples, out var seqHandle);
            if (ret == ReturnCode.NoData)
                return Int2Dds.Internal.EmptyArrayHolder<(Sample<T>, SampleInfo)>.Value;
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
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_datareader_wait_for_historical_data(_handle, (long)timeout.TotalMilliseconds));
        }

        /// <summary>
        /// Gets the subscription matched status.
        /// </summary>
        /// <returns>A tuple of (totalCount, currentCount) indicating matched writers.</returns>
        public (int totalCount, int currentCount) GetSubscriptionMatchedStatus()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_get_subscription_matched_status(_handle, out var total, out var current));
            return (total, current);
        }

        /// <summary>
        /// Gets the liveliness changed status.
        /// </summary>
        public LivelinessChangedStatus GetLivelinessChangedStatus()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

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
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

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
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

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
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

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
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

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
        /// Gets the current QoS policies of this DataReader.
        /// </summary>
        public DataReaderQos GetQos()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

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

        /// <summary>
        /// Sets new QoS policies on this DataReader.
        /// Some policies can only be changed before the entity is enabled.
        /// </summary>
        /// <param name="qos">The new QoS policies to apply.</param>
        public void SetQos(DataReaderQos qos)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

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
                        _listenerContextHandle = IntPtr.Zero;
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
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_datareader_get_statuscondition(_handle, out var conditionHandle));
            return new StatusCondition(conditionHandle);
        }

        // -- Private helpers --------------------------------------------------

        private byte[] CopyBuffer(int length)
        {
            var result = new byte[length];
            Buffer.BlockCopy(_buffer, 0, result, 0, length);
            return result;
        }

        private Sample<T>? TakeOneSample()
        {
            unsafe
            {
                fixed (byte* pBuffer = _buffer)
                {
                    var ret = NativeMethods.int2dds_take_serialized(
                        _handle, pBuffer, (UIntPtr)_buffer.Length, out var actualSize, out var validData);

                    if (ret == ReturnCode.NoData)
                        return null;
                    ReturnCodeHelper.CheckReturn(ret);

                    if (validData)
                    {
                        var data = s_deserializer(CopyBuffer((int)actualSize));
                        return new Sample<T>(data, true);
                    }
                    return new Sample<T>(default, false);
                }
            }
        }

        private Sample<T>? ReadOneSample()
        {
            unsafe
            {
                fixed (byte* pBuffer = _buffer)
                {
                    var ret = NativeMethods.int2dds_read_serialized(
                        _handle, pBuffer, (UIntPtr)_buffer.Length, out var actualSize, out var validData);

                    if (ret == ReturnCode.NoData)
                        return null;
                    ReturnCodeHelper.CheckReturn(ret);

                    if (validData)
                    {
                        var data = s_deserializer(CopyBuffer((int)actualSize));
                        return new Sample<T>(data, true);
                    }
                    return new Sample<T>(default, false);
                }
            }
        }

        private (Sample<T> Sample, SampleInfo Info)? TakeOneSampleWithInfo()
        {
            unsafe
            {
                fixed (byte* pBuffer = _buffer)
                {
                    NativeSampleInfo nativeInfo;
                    var ret = NativeMethods.int2dds_take_serialized_w_info(
                        _handle, pBuffer, (UIntPtr)_buffer.Length, out var actualSize, &nativeInfo);

                    if (ret == ReturnCode.NoData)
                        return null;
                    ReturnCodeHelper.CheckReturn(ret);

                    var info = ConvertSampleInfo(ref nativeInfo);
                    if (nativeInfo.ValidData)
                    {
                        var data = s_deserializer(CopyBuffer((int)actualSize));
                        return (new Sample<T>(data, true), info);
                    }
                    return (new Sample<T>(default, false), info);
                }
            }
        }

        private (Sample<T> Sample, SampleInfo Info)? ReadOneSampleWithInfo()
        {
            unsafe
            {
                fixed (byte* pBuffer = _buffer)
                {
                    NativeSampleInfo nativeInfo;
                    var ret = NativeMethods.int2dds_read_serialized_w_info(
                        _handle, pBuffer, (UIntPtr)_buffer.Length, out var actualSize, &nativeInfo);

                    if (ret == ReturnCode.NoData)
                        return null;
                    ReturnCodeHelper.CheckReturn(ret);

                    var info = ConvertSampleInfo(ref nativeInfo);
                    if (nativeInfo.ValidData)
                    {
                        var data = s_deserializer(CopyBuffer((int)actualSize));
                        return (new Sample<T>(data, true), info);
                    }
                    return (new Sample<T>(default, false), info);
                }
            }
        }

        private IReadOnlyList<(Sample<T> Sample, SampleInfo Info)> ReadSampleSequence(IntPtr seqHandle)
        {
            var length = (int)NativeMethods.int2dds_sample_seq_length(seqHandle);
            var results = new List<(Sample<T>, SampleInfo)>(length);

            unsafe
            {
                fixed (byte* pBuffer = _buffer)
                {
                    for (UIntPtr i = UIntPtr.Zero; (int)i < length; i = (UIntPtr)((int)i + 1))
                    {
                        // Get sample info
                        NativeSampleInfo nativeInfo;
                        ReturnCodeHelper.CheckReturn(
                            NativeMethods.int2dds_sample_seq_get_info(seqHandle, i, &nativeInfo));

                        var info = ConvertSampleInfo(ref nativeInfo);

                        if (nativeInfo.ValidData)
                        {
                            // Get sample data
                            ReturnCodeHelper.CheckReturn(
                                NativeMethods.int2dds_sample_seq_get_data(seqHandle, i, pBuffer,
                                    (UIntPtr)_buffer.Length, out var actualSize));

                            var data = s_deserializer(CopyBuffer((int)actualSize));
                            results.Add((new Sample<T>(data, true), info));
                        }
                        else
                        {
                            results.Add((new Sample<T>(default, false), info));
                        }
                    }
                }
            }

            return results;
        }

        private static unsafe SampleInfo ConvertSampleInfo(ref NativeSampleInfo native)
        {
            var instanceHandleBytes = new byte[16];
            var pubHandleBytes = new byte[16];

            fixed (byte* pInst = native.InstanceHandle)
            fixed (byte* pPub = native.PublicationHandle)
            {
                Marshal.Copy((IntPtr)pInst, instanceHandleBytes, 0, 16);
                Marshal.Copy((IntPtr)pPub, pubHandleBytes, 0, 16);
            }

            return new SampleInfo(
                native.SourceTimestampSec,
                native.SourceTimestampNanosec,
                native.SampleState,
                native.ViewState,
                native.InstanceState,
                new InstanceHandle(instanceHandleBytes),
                new InstanceHandle(pubHandleBytes),
                native.DisposedGenerationCount,
                native.NoWritersGenerationCount,
                native.SampleRank,
                native.GenerationRank,
                native.AbsoluteGenerationRank,
                native.ValidData);
        }

        private static void ApplyReaderQos(IntPtr qosHandle, DataReaderQos qos)
        {
            if (qos.Reliability != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_reliability(
                    qosHandle, (int)qos.Reliability.Kind, qos.Reliability.MaxBlockingTimeNs));

            if (qos.Durability != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_durability(
                    qosHandle, (int)qos.Durability.Kind));

            if (qos.History != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_history(
                    qosHandle, (int)qos.History.Kind, qos.History.Depth));

            if (qos.Ownership != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_ownership(
                    qosHandle, (int)qos.Ownership.Kind));

            if (qos.ResourceLimits != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_resource_limits(
                    qosHandle, qos.ResourceLimits.MaxSamples, qos.ResourceLimits.MaxInstances,
                    qos.ResourceLimits.MaxSamplesPerInstance));

            if (qos.DestinationOrder != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_destination_order(
                    qosHandle, (int)qos.DestinationOrder.Kind));

            if (qos.TimeBasedFilter != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_time_based_filter(
                    qosHandle, qos.TimeBasedFilter.MinimumSeparationNs));

            if (qos.LatencyBudget != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_latency_budget(
                    qosHandle, qos.LatencyBudget.DurationNs));

            if (qos.UserData != null && qos.UserData.Data != null && qos.UserData.Data.Length > 0)
            {
                unsafe
                {
                    fixed (byte* pData = qos.UserData.Data)
                    {
                        ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_user_data(
                            qosHandle, pData, (UIntPtr)qos.UserData.Data.Length));
                    }
                }
            }

            if (qos.ReaderDataLifecycle != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_reader_data_lifecycle(
                    qosHandle, qos.ReaderDataLifecycle.AutopurgeNowriterNs,
                    qos.ReaderDataLifecycle.AutopurgeDisposedNs));

            if (qos.DataRepresentation != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_data_representation(
                    qosHandle, (int)qos.DataRepresentation.Kind));

            if (qos.Deadline != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_deadline(
                    qosHandle, qos.Deadline.PeriodNs));

            if (qos.Liveliness != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_liveliness(
                    qosHandle, (int)qos.Liveliness.Kind, qos.Liveliness.LeaseDurationNs));
        }

        private static DataReaderQos ReadReaderQos(IntPtr h)
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

            if (_listenerContextHandle != IntPtr.Zero)
            {
                ListenerRegistry.FreeListener(_listenerContextHandle);
                _listenerContextHandle = IntPtr.Zero;
            }

            NativeMethods.int2dds_delete_datareader(_handle);
            ReturnBuffer(_buffer);
        }

        // ArrayPool<byte>.Shared isn't available on net45 (System.Buffers namespace).
        // On modern targets we use the pool to avoid GC pressure on hot reader paths;
        // on net45 we just allocate, since the embedded scenarios using net45 are
        // typically lower-throughput and we are committed to zero NuGet runtime deps.
        private static byte[] RentBuffer(int size)
        {
#if NET45
            return new byte[size];
#else
            return ArrayPool<byte>.Shared.Rent(size);
#endif
        }

        private static void ReturnBuffer(byte[] buffer)
        {
#if !NET45
            ArrayPool<byte>.Shared.Return(buffer);
#endif
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
}
