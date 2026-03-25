using System;
using System.Runtime.InteropServices;

namespace Int2Dds.Interop
{
    internal static partial class NativeMethods
    {
        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_guard_condition_new(out IntPtr condition_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_guard_condition_set_trigger_value(IntPtr condition, [MarshalAs(UnmanagedType.U1)] bool value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_guard_condition_get_trigger_value(IntPtr condition, [MarshalAs(UnmanagedType.U1)] out bool value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_guard_condition_delete(IntPtr condition);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_get_statuscondition(IntPtr reader, out IntPtr condition_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datawriter_get_statuscondition(IntPtr writer, out IntPtr condition_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_statuscondition_set_enabled_statuses(IntPtr condition, uint mask);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_statuscondition_get_enabled_statuses(IntPtr condition, out uint mask_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_statuscondition_get_trigger_value(IntPtr condition, [MarshalAs(UnmanagedType.U1)] out bool value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_statuscondition_delete(IntPtr condition);
    }
}
