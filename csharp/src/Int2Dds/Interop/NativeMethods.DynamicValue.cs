using System;
using System.Runtime.InteropServices;

namespace Int2Dds.Interop
{
    // FFI surface for XML-defined runtime types: the XML registry, dynamic type
    // support, the writable DynamicData setters, dynamic pub/sub endpoints and the
    // DynamicValue tree. Mirrors ffi/src/xml.rs and ffi/src/dynamic_value.rs.
    // `bool` parameters/outputs use C# `byte` to match the 1-byte Rust ABI and
    // avoid the default 4-byte Win32 BOOL marshaling.
    internal static partial class NativeMethods
    {
        // --- XML type registry ---

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_xml_type_registry_create(out IntPtr registry_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_xml_type_registry_from_file(byte* path, out IntPtr registry_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_xml_type_registry_load_file(IntPtr registry, byte* path);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_xml_type_registry_load_str(IntPtr registry, byte* xml);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_xml_type_registry_get_type_support(IntPtr registry, byte* name, out IntPtr support_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_xml_type_registry_get_type_object(IntPtr registry, byte* name, out IntPtr type_obj_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_xml_type_registry_type_count(IntPtr registry, out UIntPtr count_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_xml_type_registry_type_name(IntPtr registry, UIntPtr index, byte* buf, UIntPtr buf_len, out UIntPtr out_len);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern void int2dds_xml_type_registry_destroy(IntPtr registry);

        // --- Dynamic type support + endpoints ---

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern void int2dds_dynamic_type_support_destroy(IntPtr support);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_create_topic_dynamic(IntPtr participant, byte* topic_name, IntPtr type_support, IntPtr qos, out IntPtr topic_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_create_datawriter_dynamic(IntPtr publisher, IntPtr topic, IntPtr type_support, IntPtr qos, out IntPtr writer_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_create_datareader_dynamic(IntPtr subscriber, IntPtr topic, IntPtr type_support, IntPtr qos, out IntPtr reader_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern void int2dds_dynamic_writer_destroy(IntPtr writer);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern void int2dds_dynamic_reader_destroy(IntPtr reader);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_writer_publication_matched_count(IntPtr writer, out int count_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_reader_subscription_matched_count(IntPtr reader, out int count_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_writer_write(IntPtr writer, IntPtr data);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_reader_take(IntPtr reader, out IntPtr out_data, IntPtr out_info);

        // --- Writable DynamicData ---

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_data_create(IntPtr type_support, out IntPtr data_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_data_set_bool(IntPtr data, byte* field, byte value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_data_set_i8(IntPtr data, byte* field, sbyte value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_data_set_u8(IntPtr data, byte* field, byte value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_data_set_i16(IntPtr data, byte* field, short value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_data_set_u16(IntPtr data, byte* field, ushort value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_data_set_i32(IntPtr data, byte* field, int value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_data_set_u32(IntPtr data, byte* field, uint value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_data_set_i64(IntPtr data, byte* field, long value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_data_set_u64(IntPtr data, byte* field, ulong value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_data_set_f32(IntPtr data, byte* field, float value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_data_set_f64(IntPtr data, byte* field, double value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_data_set_char8(IntPtr data, byte* field, byte value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_data_set_string(IntPtr data, byte* field, byte* value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_data_set_value(IntPtr data, byte* field, IntPtr value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_data_get_value(IntPtr data, byte* path, out IntPtr value_out);

        // --- DynamicValue constructors ---

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_bool(byte value, out IntPtr value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_i8(sbyte value, out IntPtr value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_i16(short value, out IntPtr value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_i32(int value, out IntPtr value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_i64(long value, out IntPtr value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_u8(byte value, out IntPtr value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_u16(ushort value, out IntPtr value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_u32(uint value, out IntPtr value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_u64(ulong value, out IntPtr value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_f32(float value, out IntPtr value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_f64(double value, out IntPtr value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_byte(byte value, out IntPtr value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_bitmask(ulong value, out IntPtr value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_bitset(ulong value, out IntPtr value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_char8(byte value, out IntPtr value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_value_string(byte* value, out IntPtr value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_value_wstring(byte* value, out IntPtr value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_value_enum(byte* name, int value, out IntPtr value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_struct(IntPtr data, out IntPtr value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_sequence(out IntPtr value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_array(out IntPtr value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_map(out IntPtr value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_union(IntPtr discriminator, IntPtr value, out IntPtr value_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_push(IntPtr collection, IntPtr element);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_map_insert(IntPtr map, IntPtr key, IntPtr value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern void int2dds_dynamic_value_destroy(IntPtr value);

        // --- DynamicValue inspectors ---

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_kind(IntPtr value, out int kind_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_as_bool(IntPtr value, out byte out_value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_as_i8(IntPtr value, out sbyte out_value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_as_i16(IntPtr value, out short out_value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_as_i32(IntPtr value, out int out_value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_as_i64(IntPtr value, out long out_value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_as_u8(IntPtr value, out byte out_value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_as_u16(IntPtr value, out ushort out_value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_as_u32(IntPtr value, out uint out_value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_as_u64(IntPtr value, out ulong out_value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_as_f32(IntPtr value, out float out_value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_as_f64(IntPtr value, out double out_value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_as_char8(IntPtr value, out byte out_value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_value_as_string(IntPtr value, byte* buf, UIntPtr buf_len, out UIntPtr out_len);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_value_to_string(IntPtr value, byte* buf, UIntPtr buf_len, out UIntPtr out_len);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_dynamic_value_as_enum(IntPtr value, byte* buf, UIntPtr buf_len, out UIntPtr out_len, out int out_value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_as_bitmask(IntPtr value, out ulong out_value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_as_bitset(IntPtr value, out ulong out_value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_len(IntPtr value, out UIntPtr len_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_element(IntPtr value, UIntPtr index, out IntPtr out_value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_map_key(IntPtr value, UIntPtr index, out IntPtr out_value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_map_value(IntPtr value, UIntPtr index, out IntPtr out_value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_as_struct(IntPtr value, out IntPtr data_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_union_discriminator(IntPtr value, out IntPtr out_value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_dynamic_value_union_value(IntPtr value, out IntPtr out_value);
    }
}
