using System;
using System.Runtime.InteropServices;

namespace Int2Dds.Interop
{
    internal static partial class NativeMethods
    {
        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_create_topic(IntPtr participant, byte* topic_name, byte* dds_type_name, int extensibility, IntPtr qos, out IntPtr topic_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_create_topic_keyed(IntPtr participant, byte* topic_name, byte* dds_type_name, int extensibility, [MarshalAs(UnmanagedType.U1)] bool has_key, IntPtr qos, out IntPtr topic_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_create_topic_with_type_info(IntPtr participant, byte* topic_name, IntPtr type_info, IntPtr qos, out IntPtr topic_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_create_topic_with_profile(IntPtr participant, byte* topic_name, byte* dds_type_name, int extensibility, [MarshalAs(UnmanagedType.U1)] bool has_key,
            byte* qos_path, out IntPtr topic_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_topic_set_qos(IntPtr topic, IntPtr qos);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_topic_get_qos(IntPtr topic, out IntPtr qos_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_delete_topic(IntPtr topic);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_topic_get_name(IntPtr topic, byte* name_out, UIntPtr name_size);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_topic_get_type_name(IntPtr topic, byte* type_name_out, UIntPtr type_name_size);
    }
}
