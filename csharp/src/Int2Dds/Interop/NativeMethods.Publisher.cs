using System.Runtime.InteropServices;

namespace Int2Dds.Interop;

internal static partial class NativeMethods
{
    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_create_publisher(nint participant, out nint publisher_out);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_create_publisher_with_qos(nint participant, nint qos, out nint publisher_out);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_publisher_set_qos(nint publisher, nint qos);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_publisher_get_qos(nint publisher, out nint qos_out);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_delete_publisher(nint publisher);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_publisher_delete_contained_entities(nint publisher);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_publisher_wait_for_acknowledgments(nint publisher, long timeout_ms);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_create_datawriter(nint publisher, nint topic, nint qos, out nint writer_out);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_create_datawriter_with_listener(nint publisher, nint topic, nint qos, NativeDataWriterListener* listener, uint mask, out nint writer_out);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_datawriter_set_listener(nint writer, NativeDataWriterListener* listener, uint mask);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_datawriter_get_listener(nint writer, NativeDataWriterListener* listener_out);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_datawriter_set_qos(nint writer, nint qos);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_datawriter_get_qos(nint writer, out nint qos_out);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_delete_datawriter(nint writer);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_write_serialized(nint writer, byte* data, nuint data_len, byte* key, nuint key_len);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_write_serialized_w_timestamp(nint writer, byte* data, nuint data_len, byte* key, nuint key_len, int timestamp_sec, uint timestamp_nanosec);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_datawriter_wait_for_acknowledgments(nint writer, long timeout_ms);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_datawriter_register_instance(nint writer, byte* key, nuint key_len, byte* handle_out);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_datawriter_dispose(nint writer, byte* key, nuint key_len, byte* handle);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_datawriter_unregister_instance(nint writer, byte* key, nuint key_len, byte* handle);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_datawriter_lookup_instance(nint writer, byte* key, nuint key_len, byte* handle_out);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_datawriter_get_key_value(nint writer, byte* handle, byte* key_buf, nuint key_capacity, out nuint key_size_out);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_datawriter_assert_liveliness(nint writer);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_get_publication_matched_status(nint writer, out int total_count_out, out int current_count_out);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_datawriter_get_liveliness_lost_status(nint writer, NativeLivelinessLostStatus* status_out);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_datawriter_get_offered_deadline_missed_status(nint writer, NativeOfferedDeadlineMissedStatus* status_out);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_datawriter_get_offered_incompatible_qos_status(nint writer, NativeOfferedIncompatibleQosStatus* status_out);
}
