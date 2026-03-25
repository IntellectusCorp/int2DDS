using System;
using System.Runtime.InteropServices;

namespace Int2Dds.Interop
{
    internal static partial class NativeMethods
    {
        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_create_publisher(IntPtr participant, out IntPtr publisher_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_create_publisher_with_qos(IntPtr participant, IntPtr qos, out IntPtr publisher_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_publisher_set_qos(IntPtr publisher, IntPtr qos);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_publisher_get_qos(IntPtr publisher, out IntPtr qos_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_delete_publisher(IntPtr publisher);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_publisher_delete_contained_entities(IntPtr publisher);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_publisher_wait_for_acknowledgments(IntPtr publisher, long timeout_ms);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_create_datawriter(IntPtr publisher, IntPtr topic, IntPtr qos, out IntPtr writer_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_create_datawriter_with_listener(IntPtr publisher, IntPtr topic, IntPtr qos, NativeDataWriterListener* listener, uint mask, out IntPtr writer_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_datawriter_set_listener(IntPtr writer, NativeDataWriterListener* listener, uint mask);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_datawriter_get_listener(IntPtr writer, NativeDataWriterListener* listener_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datawriter_set_qos(IntPtr writer, IntPtr qos);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datawriter_get_qos(IntPtr writer, out IntPtr qos_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_delete_datawriter(IntPtr writer);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_write_serialized(IntPtr writer, byte* data, UIntPtr data_len, byte* key, UIntPtr key_len);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_write_serialized_w_timestamp(IntPtr writer, byte* data, UIntPtr data_len, byte* key, UIntPtr key_len, int timestamp_sec, uint timestamp_nanosec);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datawriter_wait_for_acknowledgments(IntPtr writer, long timeout_ms);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_datawriter_register_instance(IntPtr writer, byte* key, UIntPtr key_len, byte* handle_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_datawriter_dispose(IntPtr writer, byte* key, UIntPtr key_len, byte* handle);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_datawriter_unregister_instance(IntPtr writer, byte* key, UIntPtr key_len, byte* handle);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_datawriter_lookup_instance(IntPtr writer, byte* key, UIntPtr key_len, byte* handle_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_datawriter_get_key_value(IntPtr writer, byte* handle, byte* key_buf, UIntPtr key_capacity, out UIntPtr key_size_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datawriter_assert_liveliness(IntPtr writer);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_get_publication_matched_status(IntPtr writer, out int total_count_out, out int current_count_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_datawriter_get_liveliness_lost_status(IntPtr writer, NativeLivelinessLostStatus* status_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_datawriter_get_offered_deadline_missed_status(IntPtr writer, NativeOfferedDeadlineMissedStatus* status_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_datawriter_get_offered_incompatible_qos_status(IntPtr writer, NativeOfferedIncompatibleQosStatus* status_out);
    }
}
