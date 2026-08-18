using System.Runtime.InteropServices;

namespace Int2Dds.Interop
{
    internal static partial class NativeMethods
    {
        // ── ABI identity ────────────────────────────────────────────────────

        // Packed 0x00MMmmpp. While the library is 0.x the compatibility boundary is
        // major.minor, not major alone; see the header for the rule.
        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern uint int2dds_abi_version();

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern ulong int2dds_abi_capabilities();
    }
}
