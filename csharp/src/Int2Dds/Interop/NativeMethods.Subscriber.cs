using System.Runtime.InteropServices;

namespace Int2Dds.Interop;

internal static partial class NativeMethods
{
    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_create_subscriber(nint participant, out nint subscriber_out);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_create_subscriber_with_qos(nint participant, nint qos, out nint subscriber_out);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_subscriber_set_qos(nint subscriber, nint qos);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_subscriber_get_qos(nint subscriber, out nint qos_out);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_delete_subscriber(nint subscriber);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_subscriber_delete_contained_entities(nint subscriber);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_create_datareader(nint subscriber, nint topic, nint qos, out nint reader_out);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_create_datareader_with_listener(nint subscriber, nint topic, nint qos, NativeDataReaderListener* listener, uint mask, out nint reader_out);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_datareader_set_listener(nint reader, NativeDataReaderListener* listener, uint mask);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_datareader_get_listener(nint reader, NativeDataReaderListener* listener_out);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_datareader_set_qos(nint reader, nint qos);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_datareader_get_qos(nint reader, out nint qos_out);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_delete_datareader(nint reader);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_take_serialized(nint reader, byte* buffer, nuint buffer_capacity, out nuint actual_size_out, [MarshalAs(UnmanagedType.U1)] out bool valid_data_out);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_read_serialized(nint reader, byte* buffer, nuint buffer_capacity, out nuint actual_size_out, [MarshalAs(UnmanagedType.U1)] out bool valid_data_out);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_take_serialized_w_info(nint reader, byte* buffer, nuint buffer_capacity, out nuint actual_size_out, NativeSampleInfo* info_out);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_read_serialized_w_info(nint reader, byte* buffer, nuint buffer_capacity, out nuint actual_size_out, NativeSampleInfo* info_out);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_take_serialized_batch(nint reader, int max_samples, out nint seq_out);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_read_serialized_batch(nint reader, int max_samples, out nint seq_out);

    [LibraryImport("int2dds_ffi")]
    internal static partial nuint int2dds_sample_seq_length(nint seq);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_sample_seq_get_data(nint seq, nuint index, byte* buffer, nuint buffer_capacity, out nuint actual_size_out);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_sample_seq_get_info(nint seq, nuint index, NativeSampleInfo* info_out);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_sample_seq_delete(nint seq);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_read_serialized_w_condition(nint reader, byte* buffer, nuint buffer_capacity, out nuint actual_size_out, NativeSampleInfo* info_out, uint sample_state_mask, uint view_state_mask, uint instance_state_mask);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_take_serialized_w_condition(nint reader, byte* buffer, nuint buffer_capacity, out nuint actual_size_out, NativeSampleInfo* info_out, uint sample_state_mask, uint view_state_mask, uint instance_state_mask);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_take_serialized_batch_w_condition(nint reader, int max_samples, out nint seq_out, uint sample_state_mask, uint view_state_mask, uint instance_state_mask);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_read_serialized_batch_w_condition(nint reader, int max_samples, out nint seq_out, uint sample_state_mask, uint view_state_mask, uint instance_state_mask);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_datareader_wait_for_historical_data(nint reader, long timeout_ms);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_get_subscription_matched_status(nint reader, out int total_count_out, out int current_count_out);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_datareader_get_liveliness_changed_status(nint reader, NativeLivelinessChangedStatus* status_out);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_datareader_get_sample_rejected_status(nint reader, NativeSampleRejectedStatus* status_out);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_datareader_get_sample_lost_status(nint reader, NativeSampleLostStatus* status_out);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_datareader_get_requested_deadline_missed_status(nint reader, NativeRequestedDeadlineMissedStatus* status_out);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_datareader_get_requested_incompatible_qos_status(nint reader, NativeRequestedIncompatibleQosStatus* status_out);
}
