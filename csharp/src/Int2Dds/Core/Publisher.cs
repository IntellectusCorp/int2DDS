using System;
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
    /// Publisher - groups DataWriters for coherent publication.
    ///
    /// Publishers are created through DomainParticipant.CreatePublisher().
    /// </summary>
    public sealed class Publisher : IDisposable
    {
        private readonly IntPtr _handle;
        private bool _disposed;

        /// <summary>
        /// Creates a new Publisher. Normally called via DomainParticipant.CreatePublisher.
        /// </summary>
        internal Publisher(DomainParticipant participant, PublisherQos? qos = null)
        {
            if (qos?.Partition is { Names: { Length: var len } } partition && len > 0)
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_publisher_qos_create_default(out var qosHandle));
                try
                {
                    unsafe
                    {
                        var partitionByteArrays = partition.Names
                            .Select(n => Encoding.UTF8.GetBytes(n + '\0'))
                            .ToArray();
                        var pinnedArrays = new GCHandle[partitionByteArrays.Length];
                        for (int i = 0; i < partitionByteArrays.Length; i++)
                            pinnedArrays[i] = GCHandle.Alloc(
                                partitionByteArrays[i], GCHandleType.Pinned);

                        try
                        {
                            var ptrs = new byte*[partitionByteArrays.Length];
                            for (int i = 0; i < ptrs.Length; i++)
                                ptrs[i] = (byte*)pinnedArrays[i].AddrOfPinnedObject();
                            fixed (byte** pPartitions = ptrs)
                            {
                                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_publisher_qos_set_partition(
                                    qosHandle, pPartitions, (UIntPtr)partition.Names.Length));
                            }
                        }
                        finally
                        {
                            foreach (var pin in pinnedArrays)
                                pin.Free();
                        }
                    }

                    ReturnCodeHelper.CheckReturn(
                        NativeMethods.int2dds_create_publisher(participant.Handle, qosHandle, out _handle));
                }
                finally
                {
                    NativeMethods.int2dds_publisher_qos_destroy(qosHandle);
                }
            }
            else
            {
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_create_publisher(participant.Handle, IntPtr.Zero, out _handle));
            }
        }

        /// <summary>
        /// Creates a new Publisher using a QoS profile path.
        /// Normally called via DomainParticipant.CreatePublisherWithProfile.
        /// </summary>
        internal Publisher(DomainParticipant participant, string qosPath)
        {
            unsafe
            {
                var qosPathBytes = Encoding.UTF8.GetBytes(qosPath + '\0');
                fixed (byte* pQos = qosPathBytes)
                {
                    ReturnCodeHelper.CheckReturn(
                        NativeMethods.int2dds_create_publisher_with_profile(participant.Handle, pQos, out _handle));
                }
            }
        }

        /// <summary>
        /// Gets the native handle. For internal use by other Core types.
        /// </summary>
        internal IntPtr Handle => _handle;

        /// <summary>
        /// Gets this publisher's 16-byte instance handle.
        /// </summary>
        public unsafe byte[] GetInstanceHandle()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            var handle = new byte[16];
            fixed (byte* p = handle)
            {
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_publisher_get_instance_handle(_handle, p));
            }
            return handle;
        }

        /// <summary>
        /// Gets the StatusCondition associated with this publisher.
        /// </summary>
        public Int2Dds.Conditions.StatusCondition GetStatusCondition()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_publisher_get_statuscondition(_handle, out var conditionHandle));
            return new Int2Dds.Conditions.StatusCondition(conditionHandle);
        }

        /// <summary>
        /// Gets the current status change bitmask of this publisher.
        /// </summary>
        public uint GetStatusChanges()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_publisher_get_status_changes(_handle, out var mask));
            return mask;
        }

        /// <summary>
        /// Creates a DataWriter for the given topic.
        /// </summary>
        /// <typeparam name="T">The DDS data type.</typeparam>
        /// <param name="topic">The topic to write to.</param>
        /// <param name="qos">Optional QoS settings.</param>
        /// <param name="listener">Optional listener for event callbacks.</param>
        /// <param name="statusMask">Bitmask of statuses to listen for.</param>
        /// <returns>A new DataWriter instance.</returns>
        public DataWriter<T> CreateDataWriter<T>(Topic<T> topic, DataWriterQos? qos = null,
            IDataWriterListener? listener = null, uint statusMask = 0)
            where T : class, IDdsType, new()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            return new DataWriter<T>(this, topic, qos, listener, statusMask);
        }

        /// <summary>
        /// Creates a DataWriter using a QoS profile path (e.g. "Library::Profile").
        /// </summary>
        /// <typeparam name="T">The DDS data type.</typeparam>
        /// <param name="topic">The topic to write to.</param>
        /// <param name="qosPath">QoS profile path (e.g. "MyLibrary::MyProfile").</param>
        /// <param name="listener">Optional listener for event callbacks.</param>
        /// <param name="statusMask">Bitmask of statuses to listen for.</param>
        /// <returns>A new DataWriter instance.</returns>
        public DataWriter<T> CreateDataWriterWithProfile<T>(Topic<T> topic, string qosPath,
            IDataWriterListener listener = null, uint statusMask = 0)
            where T : class, IDdsType, new()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            return new DataWriter<T>(this, topic, qosPath, listener, statusMask);
        }

        /// <summary>
        /// Sets new QoS policies on this Publisher.
        /// Some policies can only be changed before the entity is enabled.
        /// </summary>
        /// <param name="qos">The new QoS policies to apply.</param>
        public void SetQos(PublisherQos qos)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

            // Get current QoS as base, then apply user overrides on top
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_publisher_get_qos(_handle, out var qosHandle));
            try
            {
                if (qos.Partition is { Names: { Length: var len } } partition && len > 0)
                {
                    unsafe
                    {
                        var partitionByteArrays = partition.Names
                            .Select(n => Encoding.UTF8.GetBytes(n + '\0'))
                            .ToArray();
                        var pinnedArrays = new GCHandle[partitionByteArrays.Length];
                        for (int i = 0; i < partitionByteArrays.Length; i++)
                            pinnedArrays[i] = GCHandle.Alloc(
                                partitionByteArrays[i], GCHandleType.Pinned);
                        try
                        {
                            var ptrs = new byte*[partitionByteArrays.Length];
                            for (int i = 0; i < ptrs.Length; i++)
                                ptrs[i] = (byte*)pinnedArrays[i].AddrOfPinnedObject();
                            fixed (byte** pPartitions = ptrs)
                            {
                                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_publisher_qos_set_partition(
                                    qosHandle, pPartitions, (UIntPtr)partition.Names.Length));
                            }
                        }
                        finally
                        {
                            foreach (var pin in pinnedArrays)
                                pin.Free();
                        }
                    }
                }

                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_publisher_set_qos(_handle, qosHandle));
            }
            finally
            {
                NativeMethods.int2dds_publisher_qos_destroy(qosHandle);
            }
        }

        /// <summary>
        /// Waits until all written data has been acknowledged by matched readers.
        /// </summary>
        /// <param name="timeout">Maximum time to wait.</param>
        public void WaitForAcknowledgments(TimeSpan timeout)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_publisher_wait_for_acknowledgments(_handle, (long)timeout.TotalMilliseconds));
        }

        /// <summary>
        /// Deletes all DataWriters created by this publisher.
        /// </summary>
        public void DeleteContainedEntities()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_publisher_delete_contained_entities(_handle));
        }

        /// <summary>
        /// Releases all resources used by the Publisher.
        /// </summary>
        public void Dispose()
        {
            if (_disposed) return;
            _disposed = true;
            GC.SuppressFinalize(this);

            NativeMethods.int2dds_publisher_delete_contained_entities(_handle);
            NativeMethods.int2dds_delete_publisher(_handle);
        }

        ~Publisher()
        {
            if (!_disposed)
            {
                try { Dispose(); }
                catch { /* suppress errors during finalization */ }
            }
        }
    }
}
