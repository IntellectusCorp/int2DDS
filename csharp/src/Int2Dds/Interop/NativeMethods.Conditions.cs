using System;
using System.Runtime.InteropServices;

namespace Int2Dds.Interop
{
    internal static partial class NativeMethods
    {
        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_guardcondition_new(out IntPtr condition_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_guardcondition_set_trigger_value(IntPtr condition, [MarshalAs(UnmanagedType.U1)] bool value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_guardcondition_get_trigger_value(IntPtr condition, [MarshalAs(UnmanagedType.U1)] out bool value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_guardcondition_delete(IntPtr condition);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_get_statuscondition(IntPtr reader, out IntPtr condition_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datawriter_get_statuscondition(IntPtr writer, out IntPtr condition_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_participant_get_statuscondition(IntPtr participant, out IntPtr condition_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_publisher_get_statuscondition(IntPtr publisher, out IntPtr condition_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_subscriber_get_statuscondition(IntPtr subscriber, out IntPtr condition_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_topic_get_statuscondition(IntPtr topic, out IntPtr condition_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_get_status_changes(IntPtr reader, out uint mask_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datawriter_get_status_changes(IntPtr writer, out uint mask_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_participant_get_status_changes(IntPtr participant, out uint mask_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_publisher_get_status_changes(IntPtr publisher, out uint mask_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_subscriber_get_status_changes(IntPtr subscriber, out uint mask_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_topic_get_status_changes(IntPtr topic, out uint mask_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_statuscondition_set_enabled_statuses(IntPtr condition, uint mask);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_statuscondition_get_enabled_statuses(IntPtr condition, out uint mask_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_statuscondition_get_trigger_value(IntPtr condition, [MarshalAs(UnmanagedType.U1)] out bool value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_statuscondition_delete(IntPtr condition);

        // ReadCondition / QueryCondition
        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_create_readcondition(
            IntPtr reader, uint sample_state_mask, uint view_state_mask, uint instance_state_mask, out IntPtr condition_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_datareader_create_querycondition(
            IntPtr reader, uint sample_state_mask, uint view_state_mask, uint instance_state_mask,
            byte* query_expression, byte** query_parameters, UIntPtr query_parameters_count, out IntPtr condition_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_readcondition_get_trigger_value(IntPtr condition, [MarshalAs(UnmanagedType.U1)] out bool value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_querycondition_set_query_parameters(
            IntPtr condition, byte** query_parameters, UIntPtr query_parameters_count);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_readcondition_delete(IntPtr condition);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_take_serialized_batch_w_readcondition(
            IntPtr reader, IntPtr condition, int max_samples, out IntPtr seq_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_read_serialized_batch_w_readcondition(
            IntPtr reader, IntPtr condition, int max_samples, out IntPtr seq_out);
    }
}
