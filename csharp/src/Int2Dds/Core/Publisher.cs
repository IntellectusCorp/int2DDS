using System.Text;
using Int2Dds.Exceptions;
using Int2Dds.Interop;
using Int2Dds.Listeners;
using Int2Dds.Qos;
using Int2Dds.Types;

namespace Int2Dds.Core;

/// <summary>
/// Publisher - groups DataWriters for coherent publication.
///
/// Publishers are created through DomainParticipant.CreatePublisher().
/// </summary>
public sealed class Publisher : IDisposable
{
    private readonly nint _handle;
    private bool _disposed;

    /// <summary>
    /// Creates a new Publisher. Normally called via DomainParticipant.CreatePublisher.
    /// </summary>
    internal Publisher(DomainParticipant participant, PublisherQos? qos = null)
    {
        if (qos?.Partition is { Names.Length: > 0 } partition)
        {
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_publisher_qos_create_default(out var qosHandle));
            try
            {
                unsafe
                {
                    var partitionByteArrays = partition.Names
                        .Select(n => Encoding.UTF8.GetBytes(n + '\0'))
                        .ToArray();
                    var pinnedArrays = new System.Runtime.InteropServices.GCHandle[partitionByteArrays.Length];
                    for (int i = 0; i < partitionByteArrays.Length; i++)
                        pinnedArrays[i] = System.Runtime.InteropServices.GCHandle.Alloc(
                            partitionByteArrays[i], System.Runtime.InteropServices.GCHandleType.Pinned);

                    try
                    {
                        var ptrs = new byte*[partitionByteArrays.Length];
                        for (int i = 0; i < ptrs.Length; i++)
                            ptrs[i] = (byte*)pinnedArrays[i].AddrOfPinnedObject();
                        fixed (byte** pPartitions = ptrs)
                        {
                            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_publisher_qos_set_partition(
                                qosHandle, pPartitions, (nuint)partition.Names.Length));
                        }
                    }
                    finally
                    {
                        foreach (var pin in pinnedArrays)
                            pin.Free();
                    }
                }

                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_create_publisher_with_qos(participant.Handle, qosHandle, out _handle));
            }
            finally
            {
                NativeMethods.int2dds_publisher_qos_destroy(qosHandle);
            }
        }
        else
        {
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_create_publisher(participant.Handle, out _handle));
        }
    }

    /// <summary>
    /// Gets the native handle. For internal use by other Core types.
    /// </summary>
    internal nint Handle => _handle;

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
        where T : IDdsType<T>
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
        return new DataWriter<T>(this, topic, qos, listener, statusMask);
    }

    /// <summary>
    /// Waits until all written data has been acknowledged by matched readers.
    /// </summary>
    /// <param name="timeout">Maximum time to wait.</param>
    public void WaitForAcknowledgments(TimeSpan timeout)
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
        ReturnCodeHelper.CheckReturn(
            NativeMethods.int2dds_publisher_wait_for_acknowledgments(_handle, (long)timeout.TotalMilliseconds));
    }

    /// <summary>
    /// Deletes all DataWriters created by this publisher.
    /// </summary>
    public void DeleteContainedEntities()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
        ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_publisher_delete_contained_entities(_handle));
    }

    /// <summary>
    /// Releases all resources used by the Publisher.
    /// </summary>
    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;

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
