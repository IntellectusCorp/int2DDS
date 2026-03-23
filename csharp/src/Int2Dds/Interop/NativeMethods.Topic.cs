using System.Runtime.InteropServices;

namespace Int2Dds.Interop;

internal static partial class NativeMethods
{
    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_create_topic(nint participant, byte* topic_name, byte* dds_type_name, int extensibility, nint qos, out nint topic_out);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_create_topic_keyed(nint participant, byte* topic_name, byte* dds_type_name, int extensibility, [MarshalAs(UnmanagedType.U1)] bool has_key, nint qos, out nint topic_out);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_create_topic_with_type_info(nint participant, byte* topic_name, nint type_info, nint qos, out nint topic_out);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_delete_topic(nint topic);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_topic_get_name(nint topic, byte* name_out, nuint name_size);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_topic_get_type_name(nint topic, byte* type_name_out, nuint type_name_size);
}
