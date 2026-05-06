using System;
using System.Text;
using Int2Dds.Exceptions;
using Int2Dds.Interop;
using Int2Dds.Qos;
using Int2Dds.Types;

namespace Int2Dds.Core
{
    /// <summary>
    /// DomainParticipant - the main entry point for DDS communication.
    ///
    /// A DomainParticipant represents the local membership of the application
    /// in a DDS domain. It acts as a factory for Publisher, Subscriber, and Topic.
    /// </summary>
    public sealed class DomainParticipant : IDisposable
    {
        private readonly IntPtr _handle;
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
        /// Creates a new DomainParticipant using a QoS profile path.
        /// </summary>
        /// <param name="domainId">The DDS domain to join.</param>
        /// <param name="qosPath">QoS profile path (e.g. "MyLibrary::MyProfile").</param>
        /// <param name="name">Optional name for the participant.</param>
        public DomainParticipant(int domainId, string qosPath, string? name = null)
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
                            NativeMethods.int2dds_create_participant_with_profile(factory.Handle, p, domainId, qosPath, out _handle));
                    }
                }
                else
                {
                    ReturnCodeHelper.CheckReturn(
                        NativeMethods.int2dds_create_participant_with_profile(factory.Handle, null, domainId, qosPath, out _handle));
                }
            }
        }

        /// <summary>
        /// Creates a new DomainParticipant with the given QoS settings.
        /// </summary>
        /// <param name="domainId">The DDS domain to join.</param>
        /// <param name="qos">Participant QoS (e.g. multicast TTL via PropertyQosPolicy).</param>
        /// <param name="name">Optional name for the participant.</param>
        public DomainParticipant(int domainId, ParticipantQos qos, string? name = null)
        {
            if (qos == null) throw new ArgumentNullException(nameof(qos));
            _domainId = domainId;
            var factory = DomainParticipantFactory.Instance;

            var qosHandle = BuildNativeQos(qos);
            try
            {
                unsafe
                {
                    if (name != null)
                    {
                        var nameBytes = Encoding.UTF8.GetBytes(name + '\0');
                        fixed (byte* p = nameBytes)
                        {
                            ReturnCodeHelper.CheckReturn(
                                NativeMethods.int2dds_create_participant_with_qos(factory.Handle, p, domainId, qosHandle, out _handle));
                        }
                    }
                    else
                    {
                        ReturnCodeHelper.CheckReturn(
                            NativeMethods.int2dds_create_participant_with_qos(factory.Handle, null, domainId, qosHandle, out _handle));
                    }
                }
            }
            finally
            {
                NativeMethods.int2dds_participant_qos_destroy(qosHandle);
            }
        }

        private static IntPtr BuildNativeQos(ParticipantQos qos)
        {
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_participant_qos_create_default(out var handle));

            try
            {
                if (qos.UserData != null && qos.UserData.Data != null && qos.UserData.Data.Length > 0)
                {
                    unsafe
                    {
                        fixed (byte* p = qos.UserData.Data)
                        {
                            ReturnCodeHelper.CheckReturn(
                                NativeMethods.int2dds_participant_qos_set_user_data(
                                    handle, p, (UIntPtr)qos.UserData.Data.Length));
                        }
                    }
                }

                if (qos.Property != null)
                {
                    foreach (var entry in qos.Property.Entries)
                    {
                        var nameBytes = Encoding.UTF8.GetBytes(entry.Name + '\0');
                        var valueBytes = Encoding.UTF8.GetBytes(entry.Value + '\0');
                        unsafe
                        {
                            fixed (byte* n = nameBytes)
                            fixed (byte* v = valueBytes)
                            {
                                ReturnCodeHelper.CheckReturn(
                                    NativeMethods.int2dds_participant_qos_add_property(
                                        handle, n, v, entry.Propagate));
                            }
                        }
                    }
                }

                return handle;
            }
            catch
            {
                NativeMethods.int2dds_participant_qos_destroy(handle);
                throw;
            }
        }

        /// <summary>
        /// Gets the native handle. For internal use by other Core types.
        /// </summary>
        internal IntPtr Handle => _handle;

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
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            return new Publisher(this, qos);
        }

        /// <summary>
        /// Creates a Publisher using a QoS profile path.
        /// </summary>
        /// <param name="qosPath">QoS profile path (e.g. "MyLibrary::MyProfile").</param>
        /// <returns>A new Publisher instance.</returns>
        public Publisher CreatePublisherWithProfile(string qosPath)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            return new Publisher(this, qosPath);
        }

        /// <summary>
        /// Creates a Subscriber for this participant.
        /// </summary>
        /// <param name="qos">Optional QoS settings.</param>
        /// <returns>A new Subscriber instance.</returns>
        public Subscriber CreateSubscriber(SubscriberQos? qos = null)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            return new Subscriber(this, qos);
        }

        /// <summary>
        /// Creates a Subscriber using a QoS profile path.
        /// </summary>
        /// <param name="qosPath">QoS profile path (e.g. "MyLibrary::MyProfile").</param>
        /// <returns>A new Subscriber instance.</returns>
        public Subscriber CreateSubscriberWithProfile(string qosPath)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            return new Subscriber(this, qosPath);
        }

        /// <summary>
        /// Creates a Topic for this participant.
        /// </summary>
        /// <typeparam name="T">The DDS data type, which must implement IDdsType.</typeparam>
        /// <param name="topicName">The name of the topic.</param>
        /// <param name="qos">Optional QoS settings.</param>
        /// <returns>A new Topic instance.</returns>
        public Topic<T> CreateTopic<T>(string topicName, TopicQos? qos = null) where T : class, IDdsType, new()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            return new Topic<T>(this, topicName, qos);
        }

        /// <summary>
        /// Creates a Topic using a QoS profile path.
        /// </summary>
        /// <typeparam name="T">The DDS data type, which must implement IDdsType.</typeparam>
        /// <param name="topicName">The name of the topic.</param>
        /// <param name="qosPath">QoS profile path (e.g. "MyLibrary::MyProfile").</param>
        /// <returns>A new Topic instance.</returns>
        public Topic<T> CreateTopicWithProfile<T>(string topicName, string qosPath) where T : class, IDdsType, new()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            return new Topic<T>(this, topicName, qosPath);
        }

        /// <summary>
        /// Asserts liveliness for MANUAL_BY_PARTICIPANT liveliness.
        /// </summary>
        public void AssertLiveliness()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_participant_assert_liveliness(_handle));
        }

        /// <summary>
        /// Deletes all entities (Publishers, Subscribers, Topics) created by this participant.
        /// </summary>
        public void DeleteContainedEntities()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_participant_delete_contained_entities(_handle));
        }

        /// <summary>
        /// Gets the handles of all discovered participants in this domain.
        /// </summary>
        /// <returns>An array of InstanceHandles for discovered participants.</returns>
        public InstanceHandle[] GetDiscoveredParticipants()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

            const int maxParticipants = 64;
            var buffer = new byte[maxParticipants * 16];

            unsafe
            {
                fixed (byte* p = buffer)
                {
                    ReturnCodeHelper.CheckReturn(
                        NativeMethods.int2dds_participant_get_discovered_participants(
                            _handle, p, (UIntPtr)maxParticipants, out var count));

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
}
