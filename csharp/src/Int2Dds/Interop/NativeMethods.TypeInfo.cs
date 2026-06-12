using System;
using System.Runtime.InteropServices;

namespace Int2Dds.Interop
{
    internal static partial class NativeMethods
    {
        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_type_info_create(byte* type_name, int extensibility, out IntPtr type_info_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_type_info_add_field(IntPtr type_info, byte* field_name, int field_type, int is_key);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_type_info_add_sequence_field(IntPtr type_info, byte* field_name, int element_type, uint bound, int is_key);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_type_info_add_array_field(IntPtr type_info, byte* field_name, int element_type, uint array_size, int is_key);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_type_info_add_named_type_field(IntPtr type_info, byte* field_name, byte* type_hash_name, int is_key);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_type_info_add_sequence_of_named_field(IntPtr type_info, byte* field_name, byte* element_hash_name, uint bound, int flags);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_type_info_add_array_of_named_field(IntPtr type_info, byte* field_name, byte* element_hash_name, uint array_size, int flags);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_type_info_to_type_object(IntPtr type_info, out IntPtr type_object_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern void int2dds_type_info_destroy(IntPtr type_info);
    }
}
