using System.Text;
using Int2Dds.Exceptions;
using Int2Dds.Interop;
using Int2Dds.Qos;
using Int2Dds.Types;

namespace Int2Dds.Core;

/// <summary>
/// DomainParticipant - the main entry point for DDS communication.
///
/// A DomainParticipant represents the local membership of the application
/// in a DDS domain. It acts as a factory for Publisher, Subscriber, and Topic.
/// </summary>
public sealed class DomainParticipant : IDisposable
{
    private readonly nint _handle;
    private readonly int _domainId;
    private bool _disposed;

    /// <summary>
    /// Creates a new DomainParticipant in the specified domain.
    /// </summary>
    /// <param name="domainId">The DDS domain to join (default 0).</param>
    /// <param name="name">Optional name for the participant.</param>
    public DomainParticipant(int domainId = 0, string? name = null)
    {
        _domainId = domainId;
        var factory = DomainParticipantFactory.Instance;

        unsafe
        {
            if (name != null)
            {
                var nameBytes = Encoding.UTF8.GetBytes(name + '\0');
                fixed (byte* p = nameBytes)
                {
                    ReturnCodeHelper.CheckReturn(
                        NativeMethods.int2dds_create_participant(factory.Handle, p, domainId, out _handle));
                }
            }
            else
            {
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_create_participant(factory.Handle, null, domainId, out _handle));
            }
        }
    }

    /// <summary>
    /// Gets the native handle. For internal use by other Core types.
    /// </summary>
    internal nint Handle => _handle;

    /// <summary>
    /// Gets the domain ID of this participant.
    /// </summary>
    public int DomainId => _domainId;

    /// <summary>
    /// Creates a Publisher for this participant.
    /// </summary>
    /// <param name="qos">Optional QoS settings.</param>
    /// <returns>A new Publisher instance.</returns>
    public Publisher CreatePublisher(PublisherQos? qos = null)
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
        return new Publisher(this, qos);
    }

    /// <summary>
    /// Creates a Subscriber for this participant.
    /// </summary>
    /// <param name="qos">Optional QoS settings.</param>
    /// <returns>A new Subscriber instance.</returns>
    public Subscriber CreateSubscriber(SubscriberQos? qos = null)
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
        return new Subscriber(this, qos);
    }

    /// <summary>
    /// Creates a Topic for this participant.
    /// </summary>
    /// <typeparam name="T">The DDS data type, which must implement IDdsType.</typeparam>
    /// <param name="topicName">The name of the topic.</param>
    /// <param name="qos">Optional QoS settings.</param>
    /// <returns>A new Topic instance.</returns>
    public Topic<T> CreateTopic<T>(string topicName, TopicQos? qos = null) where T : IDdsType<T>
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
        return new Topic<T>(this, topicName, qos);
    }

    /// <summary>
    /// Asserts liveliness for MANUAL_BY_PARTICIPANT liveliness.
    /// </summary>
    public void AssertLiveliness()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
        ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_participant_assert_liveliness(_handle));
    }

    /// <summary>
    /// Deletes all entities (Publishers, Subscribers, Topics) created by this participant.
    /// </summary>
    public void DeleteContainedEntities()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
        ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_participant_delete_contained_entities(_handle));
    }

    /// <summary>
    /// Gets the handles of all discovered participants in this domain.
    /// </summary>
    /// <returns>An array of InstanceHandles for discovered participants.</returns>
    public InstanceHandle[] GetDiscoveredParticipants()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);

        const int maxParticipants = 64;
        var buffer = new byte[maxParticipants * 16];

        unsafe
        {
            fixed (byte* p = buffer)
            {
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_participant_get_discovered_participants(
                        _handle, p, (nuint)maxParticipants, out var count));

                var handles = new InstanceHandle[(int)count];
                for (var i = 0; i < (int)count; i++)
                {
                    handles[i] = new InstanceHandle(buffer.AsSpan(i * 16, 16));
                }
                return handles;
            }
        }
    }

    /// <summary>
    /// Releases all resources used by the DomainParticipant.
    /// Deletes all contained entities first, then deletes the participant itself.
    /// </summary>
    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        GC.SuppressFinalize(this);

        NativeMethods.int2dds_participant_delete_contained_entities(_handle);
        NativeMethods.int2dds_delete_participant(_handle);
    }

    ~DomainParticipant()
    {
        if (!_disposed)
        {
            try { Dispose(); }
            catch { /* suppress errors during finalization */ }
        }
    }
}
