using System;
using System.Runtime.InteropServices;

namespace Int2Dds.Interop
{
    internal static partial class NativeMethods
    {
        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_create_participant(IntPtr factory, byte* name, int domain_id, out IntPtr participant_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_delete_participant(IntPtr participant);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_participant_assert_liveliness(IntPtr participant);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_participant_get_domain_id(IntPtr participant, out int domain_id_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_participant_delete_contained_entities(IntPtr participant);
    }
}
