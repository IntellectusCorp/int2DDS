using System;
using System.Runtime.InteropServices;

namespace Int2Dds.Interop
{
    internal static partial class NativeMethods
    {
        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_type_info_create(byte* type_name, int extensibility, out IntPtr type_info_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_type_info_add_field(IntPtr type_info, byte* field_name, int field_type, int flags);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_type_info_add_string_field(IntPtr type_info, byte* field_name, uint bound, int flags);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_type_info_add_wstring_field(IntPtr type_info, byte* field_name, uint bound, int flags);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_type_info_add_sequence_field(IntPtr type_info, byte* field_name, int element_type, uint bound, int flags);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_type_info_add_array_field(IntPtr type_info, byte* field_name, int element_type, uint array_size, int is_key);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_type_info_add_array_field_nd(IntPtr type_info, byte* field_name, int element_type, uint* dims, UIntPtr dims_len, int flags);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_type_info_add_named_type_field(IntPtr type_info, byte* field_name, byte* type_hash_name, int is_key);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_type_info_add_nested_field(IntPtr type_info, byte* field_name, IntPtr nested_type_info, int flags);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_type_info_add_sequence_of_nested_field(IntPtr type_info, byte* field_name, IntPtr element_type_info, uint bound, int flags);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_type_info_add_array_of_nested_field(IntPtr type_info, byte* field_name, IntPtr element_type_info, uint array_size, int flags);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_type_info_add_array_of_nested_field_nd(IntPtr type_info, byte* field_name, IntPtr element_type_info, uint* dims, UIntPtr dims_len, int flags);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_type_info_create_enum(byte* type_name, ushort bit_bound, out IntPtr out_type_info);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_type_info_add_enum_literal(IntPtr type_info, byte* literal_name, int value, int is_default);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_type_info_create_bitmask(byte* type_name, ushort bit_bound, out IntPtr out_type_info);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_type_info_add_bitmask_flag(IntPtr type_info, byte* flag_name, ushort position);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_type_info_add_sequence_of_named_field(IntPtr type_info, byte* field_name, byte* element_hash_name, uint bound, int flags);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_type_info_add_array_of_named_field(IntPtr type_info, byte* field_name, byte* element_hash_name, uint array_size, int flags);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_type_info_add_array_of_named_field_nd(IntPtr type_info, byte* field_name, byte* element_hash_name, uint* dims, UIntPtr dims_len, int flags);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_type_info_to_type_object(IntPtr type_info, out IntPtr type_object_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern void int2dds_type_info_destroy(IntPtr type_info);
    }
}
