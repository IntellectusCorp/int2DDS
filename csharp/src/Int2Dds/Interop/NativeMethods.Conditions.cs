using System.Runtime.InteropServices;

namespace Int2Dds.Interop;

internal static partial class NativeMethods
{
    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_guard_condition_new(out nint condition_out);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_guard_condition_set_trigger_value(nint condition, [MarshalAs(UnmanagedType.U1)] bool value);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_guard_condition_get_trigger_value(nint condition, [MarshalAs(UnmanagedType.U1)] out bool value_out);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_guard_condition_delete(nint condition);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_datareader_get_statuscondition(nint reader, out nint condition_out);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_datawriter_get_statuscondition(nint writer, out nint condition_out);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_statuscondition_set_enabled_statuses(nint condition, uint mask);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_statuscondition_get_enabled_statuses(nint condition, out uint mask_out);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_statuscondition_get_trigger_value(nint condition, [MarshalAs(UnmanagedType.U1)] out bool value_out);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_statuscondition_delete(nint condition);
}
