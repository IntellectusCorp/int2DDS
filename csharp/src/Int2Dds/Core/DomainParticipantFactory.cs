using System;
using Int2Dds.Exceptions;
using Int2Dds.Interop;
using Int2Dds.Qos;

namespace Int2Dds.Core
{
    /// <summary>
    /// Singleton factory for creating DomainParticipants.
    /// Wraps the native DomainParticipantFactory handle.
    /// </summary>
    public sealed class DomainParticipantFactory
    {
        private static readonly Lazy<DomainParticipantFactory> _instance = new Lazy<DomainParticipantFactory>(() =>
        {
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_domain_participant_factory_get_instance(out var handle));
            return new DomainParticipantFactory(handle);
        });

        /// <summary>
        /// Gets the singleton DomainParticipantFactory instance.
        /// </summary>
        public static DomainParticipantFactory Instance => _instance.Value;

        internal IntPtr Handle { get; }

        private DomainParticipantFactory(IntPtr handle)
        {
            Handle = handle;
        }

        /// <summary>
        /// Gets the factory's <c>autoenable_created_entities</c> policy (the only
        /// member of DomainParticipantFactoryQos).
        /// </summary>
        public bool GetQos()
        {
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_domain_participant_factory_get_qos(Handle, out var autoenable));
            return autoenable;
        }

        /// <summary>
        /// Sets the factory's <c>autoenable_created_entities</c> policy.
        /// </summary>
        public void SetQos(bool autoenableCreatedEntities)
        {
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_domain_participant_factory_set_qos(Handle, autoenableCreatedEntities));
        }

        /// <summary>
        /// Sets the factory's default participant QoS (used when a participant is
        /// created with default QoS). Pass <c>null</c> to reset to the built-in
        /// default.
        /// </summary>
        public void SetDefaultParticipantQos(ParticipantQos? qos)
        {
            if (qos == null)
            {
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_domain_participant_factory_set_default_participant_qos(Handle, IntPtr.Zero));
                return;
            }
            var qosHandle = DomainParticipant.BuildNativeQos(qos);
            try
            {
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_domain_participant_factory_set_default_participant_qos(Handle, qosHandle));
            }
            finally
            {
                NativeMethods.int2dds_participant_qos_destroy(qosHandle);
            }
        }

        /// <summary>
        /// Gets the factory's default participant QoS (properties only; user_data
        /// has no native getter).
        /// </summary>
        public ParticipantQos GetDefaultParticipantQos()
        {
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_domain_participant_factory_get_default_participant_qos(Handle, out var qosHandle));
            try
            {
                return DomainParticipant.ReadParticipantQos(qosHandle);
            }
            finally
            {
                NativeMethods.int2dds_participant_qos_destroy(qosHandle);
            }
        }

        /// <summary>
        /// Gets the resolved default participant QoS (registered default →
        /// configured default profile → spec default). Properties only.
        /// </summary>
        public ParticipantQos GetResolvedDefaultParticipantQos()
        {
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_get_default_participant_qos(Handle, out var qosHandle));
            try
            {
                return DomainParticipant.ReadParticipantQos(qosHandle);
            }
            finally
            {
                NativeMethods.int2dds_participant_qos_destroy(qosHandle);
            }
        }
    }
}
