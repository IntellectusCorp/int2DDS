using System;
using System.Runtime.InteropServices;

namespace Int2Dds.Interop
{
    internal static partial class NativeMethods
    {
        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_waitset_new(out IntPtr waitset_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_waitset_wait_ex(IntPtr waitset, long timeout_ms, out IntPtr conditions_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_waitset_wait_ex_ns(IntPtr waitset, long timeout_ns, out IntPtr conditions_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_waitset_delete(IntPtr waitset);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_waitset_attach_guardcondition(IntPtr waitset, IntPtr condition);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_waitset_detach_guardcondition(IntPtr waitset, IntPtr condition);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_waitset_attach_statuscondition(IntPtr waitset, IntPtr condition);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_waitset_detach_statuscondition(IntPtr waitset, IntPtr condition);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_waitset_attach_readcondition(IntPtr waitset, IntPtr condition);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_waitset_detach_readcondition(IntPtr waitset, IntPtr condition);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_condition_seq_length(IntPtr seq, out UIntPtr count_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_condition_seq_get(IntPtr seq, UIntPtr index, out IntPtr condition_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_condition_seq_delete(IntPtr seq);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_condition_get_trigger_value(IntPtr condition, [MarshalAs(UnmanagedType.U1)] out bool triggered_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_condition_delete(IntPtr condition);
    }
}
