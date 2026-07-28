using System;
using System.Runtime.InteropServices;

namespace Int2Dds.Interop
{
    internal static partial class NativeMethods
    {
        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_domain_participant_factory_get_instance(out IntPtr factory_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_domain_participant_factory_finalize(IntPtr factory);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_domain_participant_factory_lookup_participant(IntPtr factory, int domain_id, out IntPtr participant_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_domain_participant_factory_set_qos(IntPtr factory, [MarshalAs(UnmanagedType.U1)] bool autoenable_created_entities);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_domain_participant_factory_get_qos(IntPtr factory, [MarshalAs(UnmanagedType.U1)] out bool autoenable_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_domain_participant_factory_set_default_participant_qos(IntPtr factory, IntPtr qos);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_domain_participant_factory_get_default_participant_qos(IntPtr factory, out IntPtr qos_out);

    }
}
