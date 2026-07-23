using System.Runtime.InteropServices;

namespace Int2Dds.Interop
{
    internal static partial class NativeMethods
    {
        // ── Environment configuration ───────────────────────────────────────

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_env_set_multicast_ttl(byte ttl);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_env_get_multicast_ttl(
            out byte ttl_out,
            [MarshalAs(UnmanagedType.I1)] out bool has_value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe int int2dds_env_set_qos_profile(byte* path);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe int int2dds_env_set_default_qos_profile(byte* profile);
    }
}
