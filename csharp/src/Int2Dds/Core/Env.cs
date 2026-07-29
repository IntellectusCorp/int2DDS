using System;
using System.Text;
using Int2Dds.Exceptions;
using Int2Dds.Interop;

namespace Int2Dds.Core
{
    /// <summary>
    /// Environment-variable–driven configuration for int2DDS.
    /// </summary>
    /// <remarks>
    /// These helpers wrap the underlying <c>INT2DDS_*</c> environment variables
    /// that the Rust core consults during participant creation. They mutate the
    /// <em>current process</em> environment, so they must be called before the
    /// first <see cref="DomainParticipantFactory"/> singleton access — the
    /// factory reads <c>INT2DDS_*</c> in its <c>get_instance</c> path, while
    /// <c>INT2DDS_MULTICAST_TTL</c> is consulted at participant creation time.
    /// <para>Example:</para>
    /// <code>
    /// Int2Dds.Core.Env.SetMulticastTtl(32);   // INT2DDS_MULTICAST_TTL=32
    /// var participant = DomainParticipantFactory.Instance.CreateParticipant(...);
    /// </code>
    /// </remarks>
    public static class Env
    {
        /// <summary>
        /// Sets the IPv4 multicast TTL fallback via <c>INT2DDS_MULTICAST_TTL</c>.
        /// Used only when no explicit <c>int2dds.transport.UDPv4.multicast_ttl</c>
        /// PropertyQosPolicy entry is present, so explicit QoS settings always win.
        /// </summary>
        /// <param name="ttl">0-255.</param>
        public static void SetMulticastTtl(byte ttl)
        {
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_env_set_multicast_ttl(ttl));
        }

        /// <summary>
        /// Reads the current <c>INT2DDS_MULTICAST_TTL</c> override.
        /// </summary>
        /// <returns>
        /// The TTL when the variable is set to a valid <c>byte</c>; <c>null</c>
        /// when unset, empty, or invalid.
        /// </returns>
        public static byte? GetMulticastTtl()
        {
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_env_get_multicast_ttl(out var ttl, out var hasValue));
            return hasValue ? ttl : (byte?)null;
        }

        /// <summary>
        /// Sets the QoS profile file path(s) to auto-load via <c>DDS_QOS_PROFILE</c>.
        /// The <see cref="DomainParticipantFactory"/> auto-loads these when the first
        /// participant is created; the default-QoS resolution then draws from the
        /// selected default profile. Call before creating the first participant.
        /// Multiple paths may be joined with <c>,</c> (also <c>;</c> on Windows).
        /// </summary>
        public static void SetQosProfile(string path)
        {
            unsafe
            {
                var bytes = Encoding.UTF8.GetBytes(path + '\0');
                fixed (byte* p = bytes)
                {
                    ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_env_set_qos_profile(p));
                }
            }
        }

        /// <summary>
        /// Selects the default QoS profile (<c>"Library::Profile"</c>) via
        /// <c>DDS_DEFAULT_QOS_PROFILE</c>. The <c>*_QOS_DEFAULT</c> resolution reads
        /// this at entity-creation time, so participants/publishers/writers created
        /// with default QoS draw from this profile (e.g. <c>"HelloWorldDataFrag::Reliable"</c>).
        /// </summary>
        public static void SetDefaultQosProfile(string profile)
        {
            unsafe
            {
                var bytes = Encoding.UTF8.GetBytes(profile + '\0');
                fixed (byte* p = bytes)
                {
                    ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_env_set_default_qos_profile(p));
                }
            }
        }
    }
}
