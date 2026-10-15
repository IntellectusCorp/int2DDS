using System;
using System.Collections.Generic;
using System.Linq;
using System.Runtime.InteropServices;
using System.Text;
using Int2Dds.Exceptions;
using Int2Dds.Interop;
using Int2Dds.Listeners;
using Int2Dds.Qos;
using Int2Dds.Types;

namespace Int2Dds.Core
{
    /// <summary>
    /// Subscriber - groups DataReaders for coherent subscription.
    ///
    /// Subscribers are created through DomainParticipant.CreateSubscriber().
    /// </summary>
    public sealed class Subscriber : IDisposable
    {
        private readonly IntPtr _handle;
        private readonly Dictionary<InstanceHandle, WeakReference> _readers = new Dictionary<InstanceHandle, WeakReference>();
        private bool _disposed;

        /// <summary>
        /// Creates a new Subscriber. Normally called via DomainParticipant.CreateSubscriber.
        /// </summary>
        internal Subscriber(DomainParticipant participant, SubscriberQos? qos = null)
        {
            if (qos != null && QosMarshal.HasSubscriberPolicies(qos))
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_subscriber_qos_create_default(out var qosHandle));
                try
                {
                    QosMarshal.ApplySubscriberQos(qosHandle, qos);

                    ReturnCodeHelper.CheckReturn(
                        NativeMethods.int2dds_create_subscriber(participant.Handle, qosHandle, out _handle));
                }
                finally
                {
                    NativeMethods.int2dds_subscriber_qos_destroy(qosHandle);
                }
            }
            else
            {
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_create_subscriber(participant.Handle, IntPtr.Zero, out _handle));
            }
        }

        /// <summary>
        /// Creates a new Subscriber using a QoS profile path.
        /// Normally called via DomainParticipant.CreateSubscriberWithProfile.
        /// </summary>
        internal Subscriber(DomainParticipant participant, string qosPath)
        {
            unsafe
            {
                var qosPathBytes = Encoding.UTF8.GetBytes(qosPath + '\0');
                fixed (byte* pQos = qosPathBytes)
                {
                    ReturnCodeHelper.CheckReturn(
                        NativeMethods.int2dds_create_subscriber_with_profile(participant.Handle, pQos, out _handle));
                }
            }
        }

        /// <summary>
        /// Gets the native handle. For internal use by other Core types.
        /// </summary>
        internal IntPtr Handle => _handle;

        /// <summary>
        /// Gets this subscriber's 16-byte instance handle.
        /// </summary>
        public unsafe byte[] GetInstanceHandle()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            var handle = new byte[16];
            fixed (byte* p = handle)
            {
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_subscriber_get_instance_handle(_handle, p));
            }
            return handle;
        }

        /// <summary>
        /// Gets the StatusCondition associated with this subscriber.
        /// </summary>
        public Int2Dds.Conditions.StatusCondition GetStatusCondition()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_subscriber_get_statuscondition(_handle, out var conditionHandle));
            return new Int2Dds.Conditions.StatusCondition(conditionHandle);
        }

        /// <summary>
        /// Gets the current status change bitmask of this subscriber.
        /// </summary>
        public uint GetStatusChanges()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_subscriber_get_status_changes(_handle, out var mask));
            return mask;
        }

        /// <summary>
        /// Sets new QoS policies on this Subscriber.
        /// Some policies can only be changed before the entity is enabled.
        /// </summary>
        /// <param name="qos">The new QoS policies to apply.</param>
        public void SetQos(SubscriberQos qos)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

            // Get current QoS as base, then apply user overrides on top
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_subscriber_get_qos(_handle, out var qosHandle));
            try
            {
                QosMarshal.ApplySubscriberQos(qosHandle, qos);
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_subscriber_set_qos(_handle, qosHandle));
            }
            finally
            {
                NativeMethods.int2dds_subscriber_qos_destroy(qosHandle);
            }
        }

        /// <summary>
        /// Gets the current QoS policies of this Subscriber.
        /// Partition has no native getter and is left null.
        /// </summary>
        public SubscriberQos GetQos()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_subscriber_get_qos(_handle, out var qosHandle));
            try
            {
                return QosMarshal.ReadSubscriberQos(qosHandle);
            }
            finally
            {
                NativeMethods.int2dds_subscriber_qos_destroy(qosHandle);
            }
        }

        /// <summary>
        /// Begins a sample access block. Required before reading or calling
        /// <see cref="GetDataReaders()"/> under GROUP access scope; no effect under other
        /// access scopes. Calls may be nested.
        /// </summary>
        public void BeginAccess()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_subscriber_begin_access(_handle));
        }

        /// <summary>
        /// Ends the sample access block started by <see cref="BeginAccess"/>.
        /// </summary>
        /// <exception cref="DdsPreconditionNotMetException">GROUP access scope without a matching BeginAccess call.</exception>
        public void EndAccess()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_subscriber_end_access(_handle));
        }

        /// <summary>
        /// Gets the DataReaders of this Subscriber that hold samples in any state.
        /// See <see cref="GetDataReaders(uint, uint, uint)"/>.
        /// </summary>
        public IReadOnlyList<object> GetDataReaders()
        {
            return GetDataReaders(
                Int2Dds.Conditions.SampleState.Any, Int2Dds.Conditions.ViewState.Any, Int2Dds.Conditions.InstanceState.Any);
        }

        /// <summary>
        /// Gets the DataReaders of this Subscriber that hold samples matching the state masks.
        /// Each entry is the same <c>DataReader&lt;T&gt;</c> object returned by
        /// <see cref="CreateDataReader{T}"/> or <see cref="CreateDataReaderWithProfile{T}"/>.
        /// Under GROUP access scope with ordered access the list is in sample order and a
        /// reader may appear more than once. Readers created through other paths (e.g. dynamic
        /// type readers) are not included.
        /// </summary>
        /// <param name="sampleStates">Sample state mask (<see cref="Int2Dds.Conditions.SampleState"/>).</param>
        /// <param name="viewStates">View state mask (<see cref="Int2Dds.Conditions.ViewState"/>).</param>
        /// <param name="instanceStates">Instance state mask (<see cref="Int2Dds.Conditions.InstanceState"/>).</param>
        /// <exception cref="DdsPreconditionNotMetException">GROUP access scope outside a BeginAccess/EndAccess block.</exception>
        public unsafe IReadOnlyList<object> GetDataReaders(uint sampleStates, uint viewStates, uint instanceStates)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

            // An empty buffer passes NULL, which queries the count. Grow and retry until it fits.
            var handles = Int2Dds.Internal.EmptyArrayHolder<byte>.Value;
            int count;
            while (true)
            {
                int capacity = handles.Length / 16;
                UIntPtr total;
                fixed (byte* p = handles)
                {
                    ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_subscriber_get_datareaders(
                        _handle, sampleStates, viewStates, instanceStates, p, (UIntPtr)capacity, out total));
                }
                count = (int)(uint)total;
                if (count <= capacity) break;
                handles = new byte[count * 16];
            }

            var readers = new List<object>(count);
            lock (_readers)
            {
                for (int i = 0; i < count; i++)
                {
                    var key = new InstanceHandle(new ReadOnlySpan<byte>(handles, i * 16, 16));
                    if (_readers.TryGetValue(key, out var weak) && weak.Target is object reader)
                        readers.Add(reader);
                }
            }
            return readers;
        }

        /// <summary>
        /// Invokes the on_data_available listener of every DataReader of this Subscriber.
        /// </summary>
        public void NotifyDataReaders()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_subscriber_notify_datareaders(_handle));
        }

        // Registers a reader created by this Subscriber so GetDataReaders can map its GUID back to it.
        private DataReader<T> RegisterReader<T>(DataReader<T> reader) where T : class, IDdsType, new()
        {
            var key = new InstanceHandle(reader.GetGuid());
            lock (_readers)
            {
                _readers[key] = new WeakReference(reader);
            }
            return reader;
        }

        // Removes a disposed reader and any entries whose reader has been collected.
        internal void UnregisterReader(object reader)
        {
            lock (_readers)
            {
                var stale = _readers
                    .Where(entry => entry.Value.Target is var target && (target == null || target == reader))
                    .Select(entry => entry.Key)
                    .ToList();
                foreach (var key in stale)
                    _readers.Remove(key);
            }
        }

        /// <summary>
        /// Creates a DataReader for the given topic.
        /// </summary>
        /// <typeparam name="T">The DDS data type.</typeparam>
        /// <param name="topic">The topic to read from.</param>
        /// <param name="qos">Optional QoS settings.</param>
        /// <param name="listener">Optional listener for event callbacks.</param>
        /// <param name="statusMask">Bitmask of statuses to listen for.</param>
        /// <returns>A new DataReader instance.</returns>
        public DataReader<T> CreateDataReader<T>(Topic<T> topic, DataReaderQos? qos = null,
            IDataReaderListener? listener = null, uint statusMask = 0xFFFFFFFF)
            where T : class, IDdsType, new()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            return RegisterReader(new DataReader<T>(this, topic, qos, listener, statusMask));
        }

        /// <summary>
        /// Creates a DataReader using a QoS profile path (e.g. "Library::Profile").
        /// </summary>
        /// <typeparam name="T">The DDS data type.</typeparam>
        /// <param name="topic">The topic to read from.</param>
        /// <param name="qosPath">QoS profile path (e.g. "MyLibrary::MyProfile").</param>
        /// <param name="listener">Optional listener for event callbacks.</param>
        /// <param name="statusMask">Bitmask of statuses to listen for.</param>
        /// <returns>A new DataReader instance.</returns>
        public DataReader<T> CreateDataReaderWithProfile<T>(Topic<T> topic, string qosPath,
            IDataReaderListener? listener = null, uint statusMask = 0xFFFFFFFF)
            where T : class, IDdsType, new()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            return RegisterReader(new DataReader<T>(this, topic, qosPath, listener, statusMask));
        }

        /// <summary>
        /// Deletes all DataReaders created by this subscriber.
        /// </summary>
        public void DeleteContainedEntities()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_subscriber_delete_contained_entities(_handle));
        }

        /// <summary>
        /// Releases all resources used by the Subscriber.
        /// </summary>
        public void Dispose()
        {
            if (_disposed) return;
            _disposed = true;
            GC.SuppressFinalize(this);

            NativeMethods.int2dds_subscriber_delete_contained_entities(_handle);
            NativeMethods.int2dds_delete_subscriber(_handle);
        }

        ~Subscriber()
        {
            if (!_disposed)
            {
                try { Dispose(); }
                catch { /* suppress errors during finalization */ }
            }
        }
    }
}
