using System;
using System.Reflection;
using System.Text;
using Int2Dds.Cdr;
using Int2Dds.Exceptions;
using Int2Dds.Interop;
using Int2Dds.Qos;
using Int2Dds.Types;

namespace Int2Dds.Core
{
    /// <summary>
    /// Topic - associates a name with a data type for publish/subscribe.
    ///
    /// Topics are created through DomainParticipant.CreateTopic.
    /// </summary>
    /// <typeparam name="T">The DDS data type.</typeparam>
    public sealed class Topic<T> : IDisposable where T : class, IDdsType, new()
    {
        private readonly IntPtr _handle;
        private readonly string _name;
        private readonly string _typeName;
        private bool _disposed;

        /// <summary>
        /// Creates a new Topic. Normally called via DomainParticipant.CreateTopic.
        /// </summary>
        internal Topic(DomainParticipant participant, string topicName, TopicQos? qos = null)
        {
            _name = topicName;
            var attr = typeof(T).GetCustomAttribute<DdsTypeAttribute>();
            _typeName = attr?.TypeName ?? typeof(T).Name;
            var extensibility = attr?.Extensibility ?? 0;
            var hasKey = attr?.HasKey ?? false;

            // Create Topic QoS if provided
            IntPtr qosHandle = IntPtr.Zero;
            if (qos != null)
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_topic_qos_create_default(out qosHandle));
                try
                {
                    ApplyTopicQos(qosHandle, qos);
                }
                catch
                {
                    NativeMethods.int2dds_topic_qos_destroy(qosHandle);
                    throw;
                }
            }

            try
            {
                unsafe
                {
                    var topicNameBytes = Encoding.UTF8.GetBytes(topicName + '\0');
                    var typeNameBytes = Encoding.UTF8.GetBytes(_typeName + '\0');

                    fixed (byte* pTopicName = topicNameBytes)
                    fixed (byte* pTypeName = typeNameBytes)
                    {
                        ReturnCodeHelper.CheckReturn(
                            NativeMethods.int2dds_create_topic_keyed(
                                participant.Handle,
                                pTopicName,
                                pTypeName,
                                (int)extensibility,
                                hasKey,
                                qosHandle,
                                out _handle));
                    }
                }
            }
            finally
            {
                if (qosHandle != IntPtr.Zero)
                    NativeMethods.int2dds_topic_qos_destroy(qosHandle);
            }
        }

        /// <summary>
        /// Gets the native handle. For internal use by other Core types.
        /// </summary>
        internal IntPtr Handle => _handle;

        /// <summary>
        /// Gets the topic name.
        /// </summary>
        public string Name => _name;

        /// <summary>
        /// Gets the DDS type name.
        /// </summary>
        public string TypeName => _typeName;

        /// <summary>
        /// Gets the CLR type associated with this topic.
        /// </summary>
        public Type TypeClass => typeof(T);

        private static void ApplyTopicQos(IntPtr qosHandle, TopicQos qos)
        {
            if (qos.Reliability != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_topic_qos_set_reliability(
                    qosHandle, (int)qos.Reliability.Kind, qos.Reliability.MaxBlockingTimeNs));

            if (qos.Durability != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_topic_qos_set_durability(
                    qosHandle, (int)qos.Durability.Kind));

            if (qos.History != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_topic_qos_set_history(
                    qosHandle, (int)qos.History.Kind, qos.History.Depth));

            if (qos.Deadline != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_topic_qos_set_deadline(
                    qosHandle, qos.Deadline.PeriodNs));

            if (qos.Liveliness != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_topic_qos_set_liveliness(
                    qosHandle, (int)qos.Liveliness.Kind, qos.Liveliness.LeaseDurationNs));

            if (qos.DestinationOrder != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_topic_qos_set_destination_order(
                    qosHandle, (int)qos.DestinationOrder.Kind));

            if (qos.ResourceLimits != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_topic_qos_set_resource_limits(
                    qosHandle, qos.ResourceLimits.MaxSamples, qos.ResourceLimits.MaxInstances,
                    qos.ResourceLimits.MaxSamplesPerInstance));

            if (qos.TransportPriority != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_topic_qos_set_transport_priority(
                    qosHandle, qos.TransportPriority.Value));

            if (qos.Lifespan != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_topic_qos_set_lifespan(
                    qosHandle, qos.Lifespan.DurationNs));

            if (qos.Ownership != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_topic_qos_set_ownership(
                    qosHandle, (int)qos.Ownership.Kind));

            if (qos.DataRepresentation != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_topic_qos_set_data_representation(
                    qosHandle, (int)qos.DataRepresentation.Kind));
        }

        /// <summary>
        /// Releases all resources used by the Topic.
        /// </summary>
        public void Dispose()
        {
            if (_disposed) return;
            _disposed = true;
            NativeMethods.int2dds_delete_topic(_handle);
        }

        ~Topic()
        {
            if (!_disposed)
            {
                try { Dispose(); }
                catch { /* suppress errors during finalization */ }
            }
        }
    }
}
