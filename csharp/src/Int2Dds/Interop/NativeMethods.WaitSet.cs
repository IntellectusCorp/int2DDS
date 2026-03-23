using System.Runtime.InteropServices;

namespace Int2Dds.Interop;

internal static partial class NativeMethods
{
    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_waitset_new(out nint waitset_out);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_waitset_wait(nint waitset, long timeout_ms);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_waitset_wait_ex(nint waitset, long timeout_ms, out nint conditions_out);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_waitset_delete(nint waitset);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_waitset_attach_guard_condition(nint waitset, nint condition);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_waitset_detach_guard_condition(nint waitset, nint condition);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_waitset_attach_condition(nint waitset, nint condition);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_waitset_detach_condition(nint waitset, nint condition);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_waitset_attach_datareader(nint waitset, nint reader);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_waitset_detach_datareader(nint waitset, nint reader);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_waitset_attach_datawriter(nint waitset, nint writer);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_waitset_detach_datawriter(nint waitset, nint writer);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_condition_seq_length(nint seq, out nuint count_out);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_condition_seq_get(nint seq, nuint index, out nint condition_out);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_condition_seq_delete(nint seq);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_condition_get_trigger_value(nint condition, [MarshalAs(UnmanagedType.U1)] out bool triggered_out);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_condition_delete(nint condition);
}
