using System;
using System.Collections.Generic;
using System.Reflection;
using System.Text;
using Int2Dds.Discovery;
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
        /// <param name="name">Optional label kept for API compatibility (not sent to the core).</param>
        public DomainParticipant(int domainId = 0, string? name = null)
        {
            _domainId = domainId;
            var factory = DomainParticipantFactory.Instance;

            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_create_participant(factory.Handle, domainId, IntPtr.Zero, out _handle));
        }

        /// <summary>
        /// Creates a new DomainParticipant using a QoS profile path.
        /// </summary>
        /// <param name="domainId">The DDS domain to join.</param>
        /// <param name="qosPath">QoS profile path (e.g. "MyLibrary::MyProfile").</param>
        /// <param name="name">Optional label kept for API compatibility (not sent to the core).</param>
        public DomainParticipant(int domainId, string qosPath, string? name = null)
        {
            _domainId = domainId;
            var factory = DomainParticipantFactory.Instance;

            unsafe
            {
                var qosPathBytes = Encoding.UTF8.GetBytes(qosPath + '\0');
                fixed (byte* pQos = qosPathBytes)
                {
                    ReturnCodeHelper.CheckReturn(
                        NativeMethods.int2dds_create_participant_with_profile(factory.Handle, domainId, pQos, out _handle));
                }
            }
        }

        /// <summary>
        /// Creates a new DomainParticipant with the given QoS settings.
        /// </summary>
        /// <param name="domainId">The DDS domain to join.</param>
        /// <param name="qos">Participant QoS (e.g. multicast TTL via PropertyQosPolicy).</param>
        /// <param name="name">Optional label kept for API compatibility (not sent to the core).</param>
        public DomainParticipant(int domainId, ParticipantQos qos, string? name = null)
        {
            if (qos == null) throw new ArgumentNullException(nameof(qos));
            _domainId = domainId;
            var factory = DomainParticipantFactory.Instance;

            var qosHandle = BuildNativeQos(qos);
            try
            {
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_create_participant(factory.Handle, domainId, qosHandle, out _handle));
            }
            finally
            {
                NativeMethods.int2dds_participant_qos_destroy(qosHandle);
            }
        }

        // Wraps a handle returned by the factory's lookup_participant. The handle
        // aliases an existing core participant; disposing this wrapper frees the
        // FFI box (the factory delete is idempotent, so disposing both the
        // original and a looked-up handle is safe).
        private DomainParticipant(IntPtr handle, int domainId)
        {
            _handle = handle;
            _domainId = domainId;
        }

        /// <summary>
        /// Looks up an existing participant on <paramref name="domainId"/>.
        /// Returns <c>null</c> if none exists. Note: a looked-up participant
        /// aliases the original and supports read operations, but cannot create
        /// child entities (a core limitation the binding mirrors).
        /// </summary>
        public static DomainParticipant? LookupParticipant(int domainId)
        {
            var factory = DomainParticipantFactory.Instance;
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_domain_participant_factory_lookup_participant(
                    factory.Handle, domainId, out var handle));
            if (handle == IntPtr.Zero) return null;
            return new DomainParticipant(handle, domainId);
        }

        internal static IntPtr BuildNativeQos(ParticipantQos qos)
        {
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_participant_qos_create_default(out var handle));

            try
            {
                ApplyParticipantQos(handle, qos);
                return handle;
            }
            catch
            {
                NativeMethods.int2dds_participant_qos_destroy(handle);
                throw;
            }
        }

        // Apply the managed policies onto an existing native QoS handle. Property is
        // additive (merged onto whatever the handle already holds), matching how the
        // core resolves properties and preserving values the caller did not override.
        private static void ApplyParticipantQos(IntPtr handle, ParticipantQos qos)
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
        /// Gets the StatusCondition associated with this participant.
        /// </summary>
        public Int2Dds.Conditions.StatusCondition GetStatusCondition()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_participant_get_statuscondition(_handle, out var conditionHandle));
            return new Int2Dds.Conditions.StatusCondition(conditionHandle);
        }

        /// <summary>
        /// Gets the current status change bitmask of this participant.
        /// </summary>
        public uint GetStatusChanges()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_participant_get_status_changes(_handle, out var mask));
            return mask;
        }

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
        /// Finds an existing local Topic by name, blocking up to
        /// <paramref name="timeoutMs"/> for it to appear. Use a short timeout if
        /// the topic may not exist — a negative value blocks indefinitely.
        /// </summary>
        public Topic<T> FindTopic<T>(string topicName, int timeoutMs = 0) where T : class, IDdsType, new()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            var attr = typeof(T).GetCustomAttribute<DdsTypeAttribute>();
            var typeName = attr?.TypeName ?? typeof(T).Name;
            unsafe
            {
                var topicNameBytes = Encoding.UTF8.GetBytes(topicName + '\0');
                var typeNameBytes = Encoding.UTF8.GetBytes(typeName + '\0');
                fixed (byte* pTopicName = topicNameBytes)
                fixed (byte* pTypeName = typeNameBytes)
                {
                    ReturnCodeHelper.CheckReturn(
                        NativeMethods.int2dds_participant_find_topic(
                            _handle, pTopicName, pTypeName, timeoutMs, out var topicHandle));
                    return new Topic<T>(this, topicHandle, topicName);
                }
            }
        }

        /// <summary>
        /// Gets the participant's current wall-clock time as a UTC
        /// <see cref="DateTime"/>.
        /// </summary>
        public DateTime GetCurrentTime()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_participant_get_current_time(_handle, out var sec, out var nanosec));
            var epoch = new DateTime(1970, 1, 1, 0, 0, 0, DateTimeKind.Utc);
            return epoch.AddSeconds(sec).AddTicks(nanosec / 100);
        }

        /// <summary>
        /// Returns whether an entity with the given 16-byte instance handle
        /// belongs to this participant.
        /// </summary>
        public unsafe bool ContainsEntity(byte[] handle)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            if (handle == null) throw new ArgumentNullException(nameof(handle));
            if (handle.Length != 16) throw new ArgumentException("instance handle must be 16 bytes", nameof(handle));
            fixed (byte* p = handle)
            {
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_participant_contains_entity(_handle, p, out var result));
                return result;
            }
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
        /// Snapshots the publications (remote writers) this participant has discovered.
        /// Builtin readers report remote endpoints only. Each returned item owns a
        /// native handle and must be disposed by the caller.
        /// </summary>
        /// <param name="timeoutMs">How long to wait for the builtin reader to settle (0 = no wait).</param>
        public IReadOnlyList<PublicationBuiltinTopicData> TakeDiscoveredPublications(int timeoutMs = 0)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_participant_take_discovered_publications_snapshot(_handle, timeoutMs, out var seq));
            try
            {
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_publication_builtin_topic_data_seq_length(seq, out var count));
                var result = new PublicationBuiltinTopicData[(int)(uint)count];
                for (uint i = 0; i < (uint)count; i++)
                {
                    ReturnCodeHelper.CheckReturn(
                        NativeMethods.int2dds_publication_builtin_topic_data_seq_get(seq, (UIntPtr)i, out var item));
                    result[(int)i] = new PublicationBuiltinTopicData(item);
                }
                return result;
            }
            finally
            {
                NativeMethods.int2dds_publication_builtin_topic_data_seq_delete(seq);
            }
        }

        /// <summary>
        /// Snapshots the subscriptions (remote readers) this participant has discovered.
        /// Builtin readers report remote endpoints only. Each returned item owns a
        /// native handle and must be disposed by the caller.
        /// </summary>
        /// <param name="timeoutMs">How long to wait for the builtin reader to settle (0 = no wait).</param>
        public IReadOnlyList<SubscriptionBuiltinTopicData> TakeDiscoveredSubscriptions(int timeoutMs = 0)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_participant_take_discovered_subscriptions_snapshot(_handle, timeoutMs, out var seq));
            try
            {
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_subscription_builtin_topic_data_seq_length(seq, out var count));
                var result = new SubscriptionBuiltinTopicData[(int)(uint)count];
                for (uint i = 0; i < (uint)count; i++)
                {
                    ReturnCodeHelper.CheckReturn(
                        NativeMethods.int2dds_subscription_builtin_topic_data_seq_get(seq, (UIntPtr)i, out var item));
                    result[(int)i] = new SubscriptionBuiltinTopicData(item);
                }
                return result;
            }
            finally
            {
                NativeMethods.int2dds_subscription_builtin_topic_data_seq_delete(seq);
            }
        }

        /// <summary>
        /// Gets the current QoS policies of this participant.
        /// </summary>
        public ParticipantQos GetQos()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_participant_get_qos(_handle, out var qosHandle));
            try
            {
                return ReadParticipantQos(qosHandle);
            }
            finally
            {
                NativeMethods.int2dds_participant_qos_destroy(qosHandle);
            }
        }

        /// <summary>
        /// Sets new QoS policies on this participant. The managed policies are merged
        /// onto the participant's current QoS, so unspecified properties are preserved.
        /// </summary>
        public void SetQos(ParticipantQos qos)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            if (qos == null) throw new ArgumentNullException(nameof(qos));

            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_participant_get_qos(_handle, out var qosHandle));
            try
            {
                ApplyParticipantQos(qosHandle, qos);
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_participant_set_qos(_handle, qosHandle));
            }
            finally
            {
                NativeMethods.int2dds_participant_qos_destroy(qosHandle);
            }
        }

        // Read the property collection back from a native handle. user_data has no
        // native getter (consistent with DataWriter.GetQos) and is left unset.
        internal static ParticipantQos ReadParticipantQos(IntPtr handle)
        {
            var property = new Property();
            NativeMethods.ParticipantPropertyCallback collect;
            unsafe
            {
                collect = (namePtr, valuePtr, _) =>
                {
                    property.Add(NativeString.FromCStr(namePtr), NativeString.FromCStr(valuePtr));
                    return 0;
                };
                var emptyPrefix = stackalloc byte[1];
                emptyPrefix[0] = 0;
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_participant_qos_get_properties_with_prefix(
                        handle, emptyPrefix, collect, IntPtr.Zero));
            }
            GC.KeepAlive(collect);
            return new ParticipantQos { Property = property };
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
