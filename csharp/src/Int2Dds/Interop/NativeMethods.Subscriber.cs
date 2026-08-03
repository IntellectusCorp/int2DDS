using System;
using System.Runtime.InteropServices;

namespace Int2Dds.Interop
{
    internal static partial class NativeMethods
    {
        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_create_subscriber(IntPtr participant, IntPtr qos, out IntPtr subscriber_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_create_subscriber_with_profile(IntPtr participant,
            byte* qos_path, out IntPtr subscriber_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_subscriber_set_qos(IntPtr subscriber, IntPtr qos);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_subscriber_get_qos(IntPtr subscriber, out IntPtr qos_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_delete_subscriber(IntPtr subscriber);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_subscriber_get_instance_handle(IntPtr subscriber, byte* handle_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_subscriber_delete_contained_entities(IntPtr subscriber);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_create_datareader(IntPtr subscriber, IntPtr topic, IntPtr qos, NativeDataReaderListener* listener, uint mask, out IntPtr reader_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_create_datareader_with_profile(IntPtr subscriber, IntPtr topic,
            byte* qos_path, NativeDataReaderListener* listener, uint mask, out IntPtr reader_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_datareader_set_listener(IntPtr reader, NativeDataReaderListener* listener, uint mask);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_datareader_get_listener(IntPtr reader, NativeDataReaderListener* listener_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_set_qos(IntPtr reader, IntPtr qos);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_get_qos(IntPtr reader, out IntPtr qos_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_delete_datareader(IntPtr reader);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_datareader_take_serialized(IntPtr reader, byte* buffer, UIntPtr buffer_capacity, out UIntPtr actual_size_out, [MarshalAs(UnmanagedType.U1)] out bool valid_data_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_datareader_read_serialized(IntPtr reader, byte* buffer, UIntPtr buffer_capacity, out UIntPtr actual_size_out, [MarshalAs(UnmanagedType.U1)] out bool valid_data_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_datareader_take_serialized_w_info(IntPtr reader, byte* buffer, UIntPtr buffer_capacity, out UIntPtr actual_size_out, NativeSampleInfo* info_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_datareader_read_serialized_w_info(IntPtr reader, byte* buffer, UIntPtr buffer_capacity, out UIntPtr actual_size_out, NativeSampleInfo* info_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_take_serialized_batch(IntPtr reader, int max_samples, out IntPtr seq_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_read_serialized_batch(IntPtr reader, int max_samples, out IntPtr seq_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_datareader_take_instance_serialized_batch(IntPtr reader, byte* handle, int max_samples, uint sample_state_mask, uint view_state_mask, uint instance_state_mask, out IntPtr seq_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_datareader_read_instance_serialized_batch(IntPtr reader, byte* handle, int max_samples, uint sample_state_mask, uint view_state_mask, uint instance_state_mask, out IntPtr seq_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr int2dds_sample_seq_length(IntPtr seq);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_sample_seq_get_data(IntPtr seq, UIntPtr index, byte* buffer, UIntPtr buffer_capacity, out UIntPtr actual_size_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_sample_seq_get_info(IntPtr seq, UIntPtr index, NativeSampleInfo* info_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_sample_seq_delete(IntPtr seq);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_datareader_read_serialized_w_states(IntPtr reader, byte* buffer, UIntPtr buffer_capacity, out UIntPtr actual_size_out, NativeSampleInfo* info_out, uint sample_state_mask, uint view_state_mask, uint instance_state_mask);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_datareader_take_serialized_w_states(IntPtr reader, byte* buffer, UIntPtr buffer_capacity, out UIntPtr actual_size_out, NativeSampleInfo* info_out, uint sample_state_mask, uint view_state_mask, uint instance_state_mask);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_take_serialized_batch_w_states(IntPtr reader, int max_samples, out IntPtr seq_out, uint sample_state_mask, uint view_state_mask, uint instance_state_mask);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_read_serialized_batch_w_states(IntPtr reader, int max_samples, out IntPtr seq_out, uint sample_state_mask, uint view_state_mask, uint instance_state_mask);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_wait_for_historical_data(IntPtr reader, long timeout_ms);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_datareader_get_subscription_matched_status(IntPtr reader, NativeSubscriptionMatchedStatus* status_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_datareader_get_liveliness_changed_status(IntPtr reader, NativeLivelinessChangedStatus* status_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_datareader_get_sample_rejected_status(IntPtr reader, NativeSampleRejectedStatus* status_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_datareader_get_sample_lost_status(IntPtr reader, NativeSampleLostStatus* status_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_datareader_get_requested_deadline_missed_status(IntPtr reader, NativeRequestedDeadlineMissedStatus* status_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_datareader_get_requested_incompatible_qos_status(IntPtr reader, NativeRequestedIncompatibleQosStatus* status_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_datareader_lookup_instance(IntPtr reader, byte* key, UIntPtr key_len, byte* handle_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_datareader_get_key_value(IntPtr reader, byte* handle, byte* key_buf, UIntPtr key_capacity, out UIntPtr key_size_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_datareader_take_serialized_loaned(IntPtr reader, out byte* data_out, out UIntPtr actual_size_out, [MarshalAs(UnmanagedType.U1)] out bool valid_data_out, out IntPtr loan_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_return_serialized_loan(IntPtr loan);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_datareader_get_guid(IntPtr reader, byte* guid_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_has_data(IntPtr reader, [MarshalAs(UnmanagedType.U1)] out bool has_data_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_datareader_get_requested_incompatible_type_status(IntPtr reader, NativeRequestedIncompatibleTypeStatus* status_out);
    }
}
