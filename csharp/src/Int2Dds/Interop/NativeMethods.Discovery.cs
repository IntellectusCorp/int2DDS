using System;
using System.Runtime.InteropServices;

namespace Int2Dds.Interop
{
    internal static partial class NativeMethods
    {
        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_participant_get_discovered_participants(IntPtr participant, byte* handles_out, UIntPtr capacity, out UIntPtr count_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_datawriter_get_matched_subscriptions(IntPtr writer, byte* handles_out, UIntPtr capacity, out UIntPtr count_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_datareader_get_matched_publications(IntPtr reader, byte* handles_out, UIntPtr capacity, out UIntPtr count_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_participant_get_discovered_participant_data(IntPtr participant, byte* handle, out IntPtr data_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_datawriter_get_matched_subscription_data(IntPtr writer, byte* handle, out IntPtr data_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_datareader_get_matched_publication_data(IntPtr reader, byte* handle, out IntPtr data_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_participant_builtin_topic_data_get_key(IntPtr data, byte* key_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_participant_builtin_topic_data_get_user_data(IntPtr data, byte* buf, UIntPtr capacity, out UIntPtr size_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_participant_builtin_topic_data_destroy(IntPtr data);

        // Discovered publication/subscription snapshot family
        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_participant_take_discovered_publications_snapshot(IntPtr participant, int timeout_ms, out IntPtr seq_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_participant_take_discovered_publications_snapshot_filtered(IntPtr participant, int timeout_ms, uint instance_state_mask, out IntPtr seq_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_publication_builtin_topic_data_seq_length(IntPtr seq, out UIntPtr count_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_publication_builtin_topic_data_seq_get_instance_state(IntPtr seq, UIntPtr index, out uint instance_state_out);

        // Handle of the entry at `index`, present even where the entry carries no announcement.
        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_publication_builtin_topic_data_seq_get_instance_handle(IntPtr seq, UIntPtr index, byte* handle_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_publication_builtin_topic_data_seq_get(IntPtr seq, UIntPtr index, out IntPtr data_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_publication_builtin_topic_data_seq_delete(IntPtr seq);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_participant_take_discovered_subscriptions_snapshot(IntPtr participant, int timeout_ms, out IntPtr seq_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_participant_take_discovered_subscriptions_snapshot_filtered(IntPtr participant, int timeout_ms, uint instance_state_mask, out IntPtr seq_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_subscription_builtin_topic_data_seq_length(IntPtr seq, out UIntPtr count_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_subscription_builtin_topic_data_seq_get_instance_state(IntPtr seq, UIntPtr index, out uint instance_state_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_subscription_builtin_topic_data_seq_get_instance_handle(IntPtr seq, UIntPtr index, byte* handle_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_subscription_builtin_topic_data_seq_get(IntPtr seq, UIntPtr index, out IntPtr data_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_subscription_builtin_topic_data_seq_delete(IntPtr seq);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_publication_builtin_topic_data_get_endpoint_guid(IntPtr data, byte* guid_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_publication_builtin_topic_data_get_reliability_kind(IntPtr data, out int kind_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_publication_builtin_topic_data_get_durability_kind(IntPtr data, out int kind_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_publication_builtin_topic_data_get_liveliness_kind(IntPtr data, out int kind_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_publication_builtin_topic_data_get_liveliness_lease_duration(IntPtr data, out int sec_out, out uint nanosec_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_publication_builtin_topic_data_get_deadline(IntPtr data, out int sec_out, out uint nanosec_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_publication_builtin_topic_data_get_lifespan(IntPtr data, out int sec_out, out uint nanosec_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_publication_builtin_topic_data_get_user_data(IntPtr data, byte* buf, UIntPtr capacity, out UIntPtr size_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_subscription_builtin_topic_data_get_endpoint_guid(IntPtr data, byte* guid_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_subscription_builtin_topic_data_get_reliability_kind(IntPtr data, out int kind_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_subscription_builtin_topic_data_get_durability_kind(IntPtr data, out int kind_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_subscription_builtin_topic_data_get_liveliness_kind(IntPtr data, out int kind_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_subscription_builtin_topic_data_get_liveliness_lease_duration(IntPtr data, out int sec_out, out uint nanosec_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_subscription_builtin_topic_data_get_deadline(IntPtr data, out int sec_out, out uint nanosec_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_subscription_builtin_topic_data_get_user_data(IntPtr data, byte* buf, UIntPtr capacity, out UIntPtr size_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_publication_builtin_topic_data_get_key(IntPtr data, byte* key_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_publication_builtin_topic_data_get_participant_key(IntPtr data, byte* key_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_publication_builtin_topic_data_get_topic_name(IntPtr data, byte* buf, UIntPtr capacity, out UIntPtr size_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_publication_builtin_topic_data_get_type_name(IntPtr data, byte* buf, UIntPtr capacity, out UIntPtr size_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_publication_builtin_topic_data_destroy(IntPtr data);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_subscription_builtin_topic_data_get_key(IntPtr data, byte* key_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_subscription_builtin_topic_data_get_participant_key(IntPtr data, byte* key_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_subscription_builtin_topic_data_get_topic_name(IntPtr data, byte* buf, UIntPtr capacity, out UIntPtr size_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_subscription_builtin_topic_data_get_type_name(IntPtr data, byte* buf, UIntPtr capacity, out UIntPtr size_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_subscription_builtin_topic_data_destroy(IntPtr data);

        // pub_data/sub_data are the opaque handles above, one of them null per the
        // is_alive/is_writer pair; guid points to 16 bytes. All borrowed for the call.
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        internal unsafe delegate void EndpointDiscoveryCallback(
            IntPtr ctx, int isWriter, int isAlive, IntPtr pubData, IntPtr subData, byte* guid);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_participant_set_endpoint_discovery_callback(
            IntPtr participant, EndpointDiscoveryCallback callback, IntPtr ctx);
    }
}
