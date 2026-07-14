using System;
using System.Runtime.InteropServices;

namespace Int2Dds.Interop
{
    internal static partial class NativeMethods
    {
        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_create_participant(IntPtr factory, byte* name, int domain_id, out IntPtr participant_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_create_participant_with_profile(IntPtr factory, byte* name, int domain_id,
            byte* qos_path, out IntPtr participant_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_create_participant_with_qos(IntPtr factory, byte* name, int domain_id,
            IntPtr qos, out IntPtr participant_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_delete_participant(IntPtr participant);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_participant_assert_liveliness(IntPtr participant);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_participant_get_domain_id(IntPtr participant, out int domain_id_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_participant_delete_contained_entities(IntPtr participant);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_participant_set_qos(IntPtr participant, IntPtr qos);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_participant_get_qos(IntPtr participant, out IntPtr qos_out);
    }
}
