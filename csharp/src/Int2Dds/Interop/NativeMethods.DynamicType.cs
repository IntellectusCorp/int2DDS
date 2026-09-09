using System;
using System.Runtime.InteropServices;

namespace Int2Dds.Interop
{
    [StructLayout(LayoutKind.Sequential)]
    internal struct Int2DdsMemberInfo
    {
        public uint MemberId;
        public int Kind;
        public int Flags;
    }

    internal static partial class NativeMethods
    {
        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_participant_get_builtin_subscriber(IntPtr participant, out IntPtr subscriber_out);

        // Returns an Int2DdsPublicationBuiltinTopicData handle (see NativeMethods.Discovery.cs
        // for the accessor/destroy family).
        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_subscriber_take_publication_data(IntPtr builtin_sub, byte* topic_name_filter, int timeout_ms, out IntPtr data_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_publication_builtin_topic_data_take_type_object(IntPtr data, out IntPtr type_object_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_participant_wait_for_type_object(IntPtr participant, byte* topic_name, int timeout_ms, out IntPtr type_obj_out, byte* type_name_buf, UIntPtr type_name_buf_len, out UIntPtr out_len);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern void int2dds_type_object_destroy(IntPtr t);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_type_object_extensibility(IntPtr t, out int kind);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_type_object_member_count(IntPtr t, out uint count);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_type_object_member_info(IntPtr t, uint index, out Int2DdsMemberInfo info);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_type_object_member_name(IntPtr t, uint index, byte* buf, UIntPtr buf_len, out UIntPtr out_len);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_type_object_find_member(IntPtr t, byte* name, out uint index_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_create_topic_with_type_object(IntPtr participant, byte* topic_name, byte* type_name, IntPtr type_obj, IntPtr qos, out IntPtr topic_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_data_from_sample(IntPtr participant, byte* bytes, UIntPtr len, IntPtr type_obj, out IntPtr data_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern void int2dds_dynamic_data_destroy(IntPtr d);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_data_get_bool(IntPtr data, byte* field_path, out byte value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_data_get_i8(IntPtr data, byte* field_path, out sbyte value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_data_get_u8(IntPtr data, byte* field_path, out byte value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_data_get_i16(IntPtr data, byte* field_path, out short value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_data_get_u16(IntPtr data, byte* field_path, out ushort value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_data_get_i32(IntPtr data, byte* field_path, out int value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_data_get_u32(IntPtr data, byte* field_path, out uint value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_data_get_i64(IntPtr data, byte* field_path, out long value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_data_get_u64(IntPtr data, byte* field_path, out ulong value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_data_get_f32(IntPtr data, byte* field_path, out float value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_data_get_f64(IntPtr data, byte* field_path, out double value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_data_get_char8(IntPtr data, byte* field_path, out byte value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_data_get_string(IntPtr data, byte* field_path, byte* out_buf, UIntPtr buf_cap, out UIntPtr out_len);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_data_get_len(IntPtr data, byte* field_path, out UIntPtr len_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_data_get_member(IntPtr data, byte* field_path, out IntPtr data_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_sample_get_bool(byte* bytes, UIntPtr len, IntPtr type_obj, byte* field_name, [MarshalAs(UnmanagedType.U1)] out bool value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_sample_get_i8(byte* bytes, UIntPtr len, IntPtr type_obj, byte* field_name, out sbyte value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_sample_get_u8(byte* bytes, UIntPtr len, IntPtr type_obj, byte* field_name, out byte value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_sample_get_byte(byte* bytes, UIntPtr len, IntPtr type_obj, byte* field_name, out byte value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_sample_get_char8(byte* bytes, UIntPtr len, IntPtr type_obj, byte* field_name, out byte value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_sample_get_i16(byte* bytes, UIntPtr len, IntPtr type_obj, byte* field_name, out short value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_sample_get_u16(byte* bytes, UIntPtr len, IntPtr type_obj, byte* field_name, out ushort value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_sample_get_i32(byte* bytes, UIntPtr len, IntPtr type_obj, byte* field_name, out int value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_sample_get_u32(byte* bytes, UIntPtr len, IntPtr type_obj, byte* field_name, out uint value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_sample_get_i64(byte* bytes, UIntPtr len, IntPtr type_obj, byte* field_name, out long value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_sample_get_u64(byte* bytes, UIntPtr len, IntPtr type_obj, byte* field_name, out ulong value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_sample_get_f32(byte* bytes, UIntPtr len, IntPtr type_obj, byte* field_name, out float value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_sample_get_f64(byte* bytes, UIntPtr len, IntPtr type_obj, byte* field_name, out double value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_sample_get_string(byte* bytes, UIntPtr len, IntPtr type_obj, byte* field_name, byte* out_buf, UIntPtr buf_cap, out UIntPtr out_len);
    }
}
