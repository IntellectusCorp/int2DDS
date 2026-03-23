using System.Runtime.InteropServices;

namespace Int2Dds.Interop;

internal static partial class NativeMethods
{
    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_type_info_create(byte* type_name, int extensibility, out nint type_info_out);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_type_info_add_field(nint type_info, byte* field_name, int field_type, int is_key);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_type_info_add_sequence_field(nint type_info, byte* field_name, int element_type, uint bound, int is_key);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_type_info_add_array_field(nint type_info, byte* field_name, int element_type, uint array_size, int is_key);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_type_info_add_named_type_field(nint type_info, byte* field_name, byte* type_hash_name, int is_key);

    [LibraryImport("int2dds_ffi")]
    internal static partial void int2dds_type_info_destroy(nint type_info);
}
