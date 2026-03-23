using System.Runtime.InteropServices;

namespace Int2Dds.Interop;

internal static partial class NativeMethods
{
    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_participant_get_discovered_participants(nint participant, byte* handles_out, nuint capacity, out nuint count_out);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_datawriter_get_matched_subscriptions(nint writer, byte* handles_out, nuint capacity, out nuint count_out);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_datareader_get_matched_publications(nint reader, byte* handles_out, nuint capacity, out nuint count_out);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_participant_get_discovered_participant_data(nint participant, byte* handle, out nint data_out);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_datawriter_get_matched_subscription_data(nint writer, byte* handle, out nint data_out);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_datareader_get_matched_publication_data(nint reader, byte* handle, out nint data_out);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_participant_builtin_topic_data_get_key(nint data, byte* key_out);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_participant_builtin_topic_data_get_user_data(nint data, byte* buf, nuint capacity, out nuint size_out);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_participant_builtin_topic_data_destroy(nint data);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_publication_builtin_topic_data_get_key(nint data, byte* key_out);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_publication_builtin_topic_data_get_participant_key(nint data, byte* key_out);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_publication_builtin_topic_data_get_topic_name(nint data, byte* buf, nuint capacity, out nuint size_out);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_publication_builtin_topic_data_get_type_name(nint data, byte* buf, nuint capacity, out nuint size_out);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_publication_builtin_topic_data_destroy(nint data);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_subscription_builtin_topic_data_get_key(nint data, byte* key_out);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_subscription_builtin_topic_data_get_participant_key(nint data, byte* key_out);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_subscription_builtin_topic_data_get_topic_name(nint data, byte* buf, nuint capacity, out nuint size_out);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_subscription_builtin_topic_data_get_type_name(nint data, byte* buf, nuint capacity, out nuint size_out);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_subscription_builtin_topic_data_destroy(nint data);
}
