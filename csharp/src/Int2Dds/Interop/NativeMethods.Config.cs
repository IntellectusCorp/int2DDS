using System;
using System.Runtime.InteropServices;

namespace Int2Dds.Interop
{
    internal static partial class NativeMethods
    {
        // const char *const *paths marshals as an array of UTF-8 pointers.
        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_load_profiles(IntPtr[] paths, UIntPtr count);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe int int2dds_get_dynamic_type_support(
            byte* type_name, out IntPtr support_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe int int2dds_create_participant_from_config(
            IntPtr factory, byte* path, out IntPtr configured_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe int int2dds_configured_participant_get_datawriter(
            IntPtr configured, byte* name, out IntPtr writer_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe int int2dds_configured_participant_get_datareader(
            IntPtr configured, byte* name, out IntPtr reader_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern void int2dds_configured_participant_destroy(IntPtr configured);
    }
}
