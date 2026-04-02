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
    }
}
