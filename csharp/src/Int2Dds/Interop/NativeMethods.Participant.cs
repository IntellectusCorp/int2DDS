using System.Runtime.InteropServices;

namespace Int2Dds.Interop;

internal static partial class NativeMethods
{
    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_create_participant(nint factory, byte* name, int domain_id, out nint participant_out);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_delete_participant(nint participant);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_participant_assert_liveliness(nint participant);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_participant_get_domain_id(nint participant, out int domain_id_out);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_participant_delete_contained_entities(nint participant);
}
