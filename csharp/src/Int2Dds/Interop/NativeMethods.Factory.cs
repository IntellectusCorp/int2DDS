using System.Runtime.InteropServices;

namespace Int2Dds.Interop;

internal static partial class NativeMethods
{
    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_domain_participant_factory_get_instance(out nint factory_out);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_domain_participant_factory_finalize(nint factory);
}
