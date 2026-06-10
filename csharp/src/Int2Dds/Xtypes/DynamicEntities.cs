using System;
using System.Text;
using Int2Dds.Conditions;
using Int2Dds.Core;
using Int2Dds.Exceptions;
using Int2Dds.Interop;
using Int2Dds.Qos;
using Int2Dds.TypeInfo;

namespace Int2Dds.Xtypes
{
    /// <summary>
    /// A topic created from a runtime type definition (a <see cref="TypeInfoBuilder"/>
    /// on the publisher side, or a discovered <see cref="DynamicTypeObject"/> on the
    /// subscriber side) rather than a compile-time <c>IDdsType</c>.
    ///
    /// Use with <see cref="DynamicDataWriter"/> / <see cref="DynamicDataReader"/> to
    /// publish and receive samples whose type is only known at runtime.
    /// </summary>
    public sealed class DynamicTopic : IDisposable
    {
        private IntPtr _handle;
        private bool _disposed;

        internal DynamicTopic(IntPtr handle, string name)
        {
            _handle = handle;
            Name = name;
        }

        internal IntPtr Handle => _handle;

        /// <summary>Gets the topic name.</summary>
        public string Name { get; }

        public void Dispose()
        {
            if (_disposed) return;
            _disposed = true;
            if (_handle != IntPtr.Zero)
            {
                NativeMethods.int2dds_delete_topic(_handle);
                _handle = IntPtr.Zero;
            }
        }
    }

    /// <summary>
    /// A DataWriter for a <see cref="DynamicTopic"/>. Instead of a typed sample, it
    /// publishes raw CDR-serialized bytes (e.g. produced by <c>Int2Dds.Cdr.CdrWriter</c>),
    /// which lets a publisher emit data for a type defined purely at runtime.
    /// </summary>
    public sealed class DynamicDataWriter : IDisposable
    {
        private IntPtr _handle;
        private bool _disposed;

        internal DynamicDataWriter(Publisher publisher, DynamicTopic topic,
            ReliabilityKind reliability, long maxBlockingTimeNs)
        {
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_create_default(out IntPtr qos));
            try
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_reliability(
                    qos, (int)reliability, maxBlockingTimeNs));
                // The serialized samples use classic (XCDR1) CDR; advertise the same in SEDP.
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_data_representation(
                    qos, (int)DataRepresentationKind.Xcdr1));
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_create_datawriter(
                    publisher.Handle, topic.Handle, qos, out _handle));
            }
            finally
            {
                NativeMethods.int2dds_datawriter_qos_destroy(qos);
            }
        }

        internal IntPtr Handle => _handle;

        /// <summary>Gets the current number of matched readers.</summary>
        public int MatchedReaders
        {
            get
            {
                if (_disposed) throw new ObjectDisposedException(GetType().Name);
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_get_publication_matched_status(_handle, out _, out int current));
                return current;
            }
        }

        /// <summary>
        /// Publishes a pre-serialized CDR sample.
        /// </summary>
        /// <param name="data">The CDR-serialized sample bytes (including the encapsulation header).</param>
        /// <param name="key">The CDR-serialized key bytes, or <c>null</c> for a keyless write.</param>
        public unsafe void WriteSerialized(byte[] data, byte[]? key = null)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            if (data == null) throw new ArgumentNullException(nameof(data));

            fixed (byte* pData = data)
            fixed (byte* pKey = key)
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_write_serialized(
                    _handle,
                    pData, (UIntPtr)data.Length,
                    pKey, key != null ? (UIntPtr)key.Length : UIntPtr.Zero));
            }
        }

        /// <summary>Gets the StatusCondition associated with this writer.</summary>
        public StatusCondition GetStatusCondition()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_datawriter_get_statuscondition(_handle, out IntPtr conditionHandle));
            return new StatusCondition(conditionHandle);
        }

        public void Dispose()
        {
            if (_disposed) return;
            _disposed = true;
            if (_handle != IntPtr.Zero)
            {
                NativeMethods.int2dds_delete_datawriter(_handle);
                _handle = IntPtr.Zero;
            }
        }
    }

    /// <summary>
    /// A DataReader for a <see cref="DynamicTopic"/>. It returns raw CDR-serialized
    /// sample bytes, which can be decoded with
    /// <see cref="DynamicSupport.DecodeSample"/> into a <see cref="DynamicData"/>.
    /// </summary>
    public sealed class DynamicDataReader : IDisposable
    {
        private const int BufferSize = 65536;

        private IntPtr _handle;
        private readonly byte[] _buffer = new byte[BufferSize];
        private bool _disposed;

        internal DynamicDataReader(Subscriber subscriber, DynamicTopic topic,
            ReliabilityKind reliability, long maxBlockingTimeNs)
        {
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_create_default(out IntPtr qos));
            try
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_reliability(
                    qos, (int)reliability, maxBlockingTimeNs));
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_data_representation(
                    qos, (int)DataRepresentationKind.Xcdr1));
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_create_datareader(
                    subscriber.Handle, topic.Handle, qos, out _handle));
            }
            finally
            {
                NativeMethods.int2dds_datareader_qos_destroy(qos);
            }
        }

        internal IntPtr Handle => _handle;

        /// <summary>Gets the current number of matched writers.</summary>
        public int MatchedWriters
        {
            get
            {
                if (_disposed) throw new ObjectDisposedException(GetType().Name);
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_get_subscription_matched_status(_handle, out _, out int current));
                return current;
            }
        }

        /// <summary>
        /// Takes the next available sample, returning its raw CDR-serialized bytes,
        /// or <c>null</c> if no valid data is available.
        /// </summary>
        public unsafe byte[]? TakeNextSample()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

            fixed (byte* pBuffer = _buffer)
            {
                int ret = NativeMethods.int2dds_take_serialized(
                    _handle, pBuffer, (UIntPtr)_buffer.Length, out UIntPtr actualSize, out bool validData);

                if (ret == ReturnCode.NoData)
                    return null;
                ReturnCodeHelper.CheckReturn(ret);
                if (!validData)
                    return null;

                var result = new byte[(int)actualSize];
                Buffer.BlockCopy(_buffer, 0, result, 0, (int)actualSize);
                return result;
            }
        }

        /// <summary>Gets the StatusCondition associated with this reader.</summary>
        public StatusCondition GetStatusCondition()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_datareader_get_statuscondition(_handle, out IntPtr conditionHandle));
            return new StatusCondition(conditionHandle);
        }

        public void Dispose()
        {
            if (_disposed) return;
            _disposed = true;
            if (_handle != IntPtr.Zero)
            {
                NativeMethods.int2dds_delete_datareader(_handle);
                _handle = IntPtr.Zero;
            }
        }
    }

    /// <summary>
    /// Participant/Publisher/Subscriber extension methods for creating dynamic
    /// (runtime-typed) topics, writers and readers.
    /// </summary>
    public static class DynamicEntities
    {
        // int2dds_wait_for_type_object returns this when no matching type is
        // discovered within the timeout (INT2DDS_RET_DYNAMIC_TIMEOUT). It is the
        // expected outcome while polling for a not-yet-present publisher.
        private const int DynamicTimeout = 203;

        /// <summary>
        /// Like <see cref="DynamicSupport.WaitForTypeObject"/>, but returns
        /// <c>null</c> (instead of throwing) when the wait times out without
        /// discovering a type — convenient for polling loops. Other failures still
        /// throw.
        /// </summary>
        public static unsafe DynamicTypeObject? TryWaitForTypeObject(
            this DomainParticipant participant, string topicName, int timeoutMs, out string? typeName)
        {
            typeName = null;
            var topicBytes = Encoding.UTF8.GetBytes(topicName + '\0');
            byte[] nameBuf = new byte[256];
            fixed (byte* pTopic = topicBytes)
            fixed (byte* pName = nameBuf)
            {
                int ret = NativeMethods.int2dds_wait_for_type_object(
                    participant.Handle, pTopic, timeoutMs, out IntPtr typeObj,
                    pName, (UIntPtr)nameBuf.Length, out UIntPtr outLen);
                if (ret == DynamicTimeout)
                    return null;
                ReturnCodeHelper.CheckReturn(ret);
                typeName = Encoding.UTF8.GetString(nameBuf, 0, (int)outLen);
                return new DynamicTypeObject(typeObj);
            }
        }

        /// <summary>
        /// Creates a topic from a <see cref="TypeInfoBuilder"/> (publisher side). The
        /// builder's TypeObject is advertised during discovery so that subscribers can
        /// learn the type at runtime. The builder is consumed (its handle ownership is
        /// transferred to the topic).
        /// </summary>
        public static unsafe DynamicTopic CreateDynamicTopic(
            this DomainParticipant participant, string topicName, TypeInfoBuilder builder)
        {
            if (builder == null) throw new ArgumentNullException(nameof(builder));

            IntPtr typeInfo = builder.Build();
            var topicBytes = Encoding.UTF8.GetBytes(topicName + '\0');
            fixed (byte* pTopic = topicBytes)
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_create_topic_with_type_info(
                    participant.Handle, pTopic, typeInfo, IntPtr.Zero, out IntPtr topic));
                return new DynamicTopic(topic, topicName);
            }
        }

        /// <summary>
        /// Creates a topic from a discovered <see cref="DynamicTypeObject"/> (subscriber
        /// side), typically obtained via <see cref="DynamicSupport.WaitForTypeObject"/>.
        /// </summary>
        public static unsafe DynamicTopic CreateDynamicTopic(
            this DomainParticipant participant, string topicName, string typeName, DynamicTypeObject typeObj)
        {
            if (typeObj == null) throw new ArgumentNullException(nameof(typeObj));

            var topicBytes = Encoding.UTF8.GetBytes(topicName + '\0');
            var typeBytes = Encoding.UTF8.GetBytes(typeName + '\0');
            fixed (byte* pTopic = topicBytes)
            fixed (byte* pType = typeBytes)
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_create_topic_with_type_object(
                    participant.Handle, pTopic, pType, typeObj.Handle, IntPtr.Zero, out IntPtr topic));
                return new DynamicTopic(topic, topicName);
            }
        }

        /// <summary>Creates a <see cref="DynamicDataWriter"/> for the given dynamic topic.</summary>
        public static DynamicDataWriter CreateDynamicDataWriter(
            this Publisher publisher, DynamicTopic topic,
            ReliabilityKind reliability = ReliabilityKind.Reliable,
            long maxBlockingTimeNs = 1_000_000_000)
        {
            if (topic == null) throw new ArgumentNullException(nameof(topic));
            return new DynamicDataWriter(publisher, topic, reliability, maxBlockingTimeNs);
        }

        /// <summary>Creates a <see cref="DynamicDataReader"/> for the given dynamic topic.</summary>
        public static DynamicDataReader CreateDynamicDataReader(
            this Subscriber subscriber, DynamicTopic topic,
            ReliabilityKind reliability = ReliabilityKind.Reliable,
            long maxBlockingTimeNs = 1_000_000_000)
        {
            if (topic == null) throw new ArgumentNullException(nameof(topic));
            return new DynamicDataReader(subscriber, topic, reliability, maxBlockingTimeNs);
        }
    }
}
