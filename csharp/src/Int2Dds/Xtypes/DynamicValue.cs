using System;
using Int2Dds.Exceptions;
using Int2Dds.Interop;

namespace Int2Dds.Xtypes
{
    /// <summary>The kind of a <see cref="DynamicValue"/> (returned by <see cref="DynamicValue.Kind"/>).</summary>
    public enum DynamicValueKind
    {
        Boolean = 0,
        Int8 = 1,
        Int16 = 2,
        Int32 = 3,
        Int64 = 4,
        UInt8 = 5,
        UInt16 = 6,
        UInt32 = 7,
        UInt64 = 8,
        Float32 = 9,
        Float64 = 10,
        Char8 = 11,
        Byte = 12,
        String = 13,
        WString = 14,
        Enum = 15,
        Union = 16,
        Bitmask = 17,
        Bitset = 18,
        Struct = 19,
        Sequence = 20,
        Array = 21,
        Map = 22,
        Optional = 23,
        Null = 24,
    }

    /// <summary>
    /// A (possibly nested) value tree mirroring the core <c>DynamicValue</c>. Build write
    /// payloads with the static constructors and <see cref="Push"/>/<see cref="MapInsert"/>;
    /// inspect read-back values with <see cref="Kind"/> and the <c>As*</c>/<see cref="Element"/>
    /// getters.
    ///
    /// Ownership: a value handed to <see cref="Push"/>/<see cref="MapInsert"/>/<see cref="Union"/>
    /// or <see cref="DynamicData.SetValue"/> is consumed on success and must not be reused.
    /// </summary>
    public sealed class DynamicValue : IDisposable
    {
        private IntPtr _handle;

        internal DynamicValue(IntPtr handle)
        {
            _handle = handle;
        }

        internal IntPtr Handle => _handle;

        internal void Consume() => _handle = IntPtr.Zero;

        private static DynamicValue Wrap(int ret, IntPtr handle)
        {
            ReturnCodeHelper.CheckReturn(ret);
            return new DynamicValue(handle);
        }

        // --- Scalar constructors ---

        public static DynamicValue Bool(bool v) => Wrap(NativeMethods.int2dds_dynamic_value_bool((byte)(v ? 1 : 0), out IntPtr h), h);
        public static DynamicValue I8(sbyte v) => Wrap(NativeMethods.int2dds_dynamic_value_i8(v, out IntPtr h), h);
        public static DynamicValue I16(short v) => Wrap(NativeMethods.int2dds_dynamic_value_i16(v, out IntPtr h), h);
        public static DynamicValue I32(int v) => Wrap(NativeMethods.int2dds_dynamic_value_i32(v, out IntPtr h), h);
        public static DynamicValue I64(long v) => Wrap(NativeMethods.int2dds_dynamic_value_i64(v, out IntPtr h), h);
        public static DynamicValue U8(byte v) => Wrap(NativeMethods.int2dds_dynamic_value_u8(v, out IntPtr h), h);
        public static DynamicValue U16(ushort v) => Wrap(NativeMethods.int2dds_dynamic_value_u16(v, out IntPtr h), h);
        public static DynamicValue U32(uint v) => Wrap(NativeMethods.int2dds_dynamic_value_u32(v, out IntPtr h), h);
        public static DynamicValue U64(ulong v) => Wrap(NativeMethods.int2dds_dynamic_value_u64(v, out IntPtr h), h);
        public static DynamicValue F32(float v) => Wrap(NativeMethods.int2dds_dynamic_value_f32(v, out IntPtr h), h);
        public static DynamicValue F64(double v) => Wrap(NativeMethods.int2dds_dynamic_value_f64(v, out IntPtr h), h);
        public static DynamicValue Byte(byte v) => Wrap(NativeMethods.int2dds_dynamic_value_byte(v, out IntPtr h), h);
        public static DynamicValue Bitmask(ulong v) => Wrap(NativeMethods.int2dds_dynamic_value_bitmask(v, out IntPtr h), h);
        public static DynamicValue Bitset(ulong v) => Wrap(NativeMethods.int2dds_dynamic_value_bitset(v, out IntPtr h), h);
        public static DynamicValue Char8(byte v) => Wrap(NativeMethods.int2dds_dynamic_value_char8(v, out IntPtr h), h);

        public static unsafe DynamicValue String(string v)
        {
            var b = NativeString.ToCStr(v);
            fixed (byte* p = b)
                return Wrap(NativeMethods.int2dds_dynamic_value_string(p, out IntPtr h), h);
        }

        public static unsafe DynamicValue WString(string v)
        {
            var b = NativeString.ToCStr(v);
            fixed (byte* p = b)
                return Wrap(NativeMethods.int2dds_dynamic_value_wstring(p, out IntPtr h), h);
        }

        public static unsafe DynamicValue Enum(string name, int value)
        {
            var b = NativeString.ToCStr(name);
            fixed (byte* p = b)
                return Wrap(NativeMethods.int2dds_dynamic_value_enum(p, value, out IntPtr h), h);
        }

        /// <summary>Build a nested struct value by cloning a <see cref="DynamicData"/>.</summary>
        public static DynamicValue Struct(DynamicData data)
        {
            if (data == null) throw new ArgumentNullException(nameof(data));
            return Wrap(NativeMethods.int2dds_dynamic_value_struct(data.Handle, out IntPtr h), h);
        }

        public static DynamicValue Sequence() => Wrap(NativeMethods.int2dds_dynamic_value_sequence(out IntPtr h), h);
        public static DynamicValue Array() => Wrap(NativeMethods.int2dds_dynamic_value_array(out IntPtr h), h);
        public static DynamicValue Map() => Wrap(NativeMethods.int2dds_dynamic_value_map(out IntPtr h), h);

        /// <summary>Build a union value from a discriminator and its selected branch. Both are consumed on success.</summary>
        public static DynamicValue Union(DynamicValue discriminator, DynamicValue value)
        {
            if (discriminator == null) throw new ArgumentNullException(nameof(discriminator));
            if (value == null) throw new ArgumentNullException(nameof(value));
            int ret = NativeMethods.int2dds_dynamic_value_union(discriminator.Handle, value.Handle, out IntPtr h);
            if (ret != ReturnCode.NullPointer)
            {
                discriminator.Consume();
                value.Consume();
            }
            ReturnCodeHelper.CheckReturn(ret);
            return new DynamicValue(h);
        }

        // --- Mutators ---

        /// <summary>Append to a sequence/array value. Consumes <paramref name="element"/> on success.</summary>
        public DynamicValue Push(DynamicValue element)
        {
            if (element == null) throw new ArgumentNullException(nameof(element));
            int ret = NativeMethods.int2dds_dynamic_value_push(_handle, element.Handle);
            if (ret == ReturnCode.Ok) element.Consume();
            ReturnCodeHelper.CheckReturn(ret);
            return this;
        }

        /// <summary>Insert a key/value pair into a map value. Consumes both on success.</summary>
        public DynamicValue MapInsert(DynamicValue key, DynamicValue value)
        {
            if (key == null) throw new ArgumentNullException(nameof(key));
            if (value == null) throw new ArgumentNullException(nameof(value));
            int ret = NativeMethods.int2dds_dynamic_value_map_insert(_handle, key.Handle, value.Handle);
            if (ret == ReturnCode.Ok) { key.Consume(); value.Consume(); }
            ReturnCodeHelper.CheckReturn(ret);
            return this;
        }

        // --- Inspectors ---

        public DynamicValueKind Kind()
        {
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_value_kind(_handle, out int k));
            return (DynamicValueKind)k;
        }

        public bool AsBool()
        {
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_value_as_bool(_handle, out byte v));
            return v != 0;
        }

        public sbyte AsI8() { ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_value_as_i8(_handle, out sbyte v)); return v; }
        public short AsI16() { ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_value_as_i16(_handle, out short v)); return v; }
        public int AsI32() { ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_value_as_i32(_handle, out int v)); return v; }
        public long AsI64() { ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_value_as_i64(_handle, out long v)); return v; }
        public byte AsU8() { ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_value_as_u8(_handle, out byte v)); return v; }
        public ushort AsU16() { ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_value_as_u16(_handle, out ushort v)); return v; }
        public uint AsU32() { ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_value_as_u32(_handle, out uint v)); return v; }
        public ulong AsU64() { ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_value_as_u64(_handle, out ulong v)); return v; }
        public float AsF32() { ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_value_as_f32(_handle, out float v)); return v; }
        public double AsF64() { ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_value_as_f64(_handle, out double v)); return v; }
        public byte AsChar8() { ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_value_as_char8(_handle, out byte v)); return v; }
        public ulong AsBitmask() { ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_value_as_bitmask(_handle, out ulong v)); return v; }
        public ulong AsBitset() { ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_value_as_bitset(_handle, out ulong v)); return v; }

        public unsafe string AsString()
        {
            IntPtr h = _handle;
            return NativeString.Read((byte* buf, UIntPtr cap, out UIntPtr len) =>
                NativeMethods.int2dds_dynamic_value_as_string(h, buf, cap, out len));
        }

        /// <summary>Format the value as a string regardless of kind (mirrors the core Display).</summary>
        public unsafe override string ToString()
        {
            IntPtr h = _handle;
            return NativeString.Read((byte* buf, UIntPtr cap, out UIntPtr len) =>
                NativeMethods.int2dds_dynamic_value_to_string(h, buf, cap, out len));
        }

        /// <summary>Read an enum value's literal name and numeric value.</summary>
        public unsafe (string Name, int Value) AsEnum()
        {
            IntPtr h = _handle;
            int value = 0;
            string name = NativeString.Read((byte* buf, UIntPtr cap, out UIntPtr len) =>
                NativeMethods.int2dds_dynamic_value_as_enum(h, buf, cap, out len, out value));
            return (name, value);
        }

        /// <summary>Element count of a sequence/array/map value.</summary>
        public int Length()
        {
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_value_len(_handle, out UIntPtr len));
            return (int)len;
        }

        private DynamicValue Child(Func<IntPtr, UIntPtr, (int ret, IntPtr handle)> fn, int index)
        {
            var (ret, handle) = fn(_handle, (UIntPtr)index);
            return Wrap(ret, handle);
        }

        public DynamicValue Element(int index) => Child((h, i) => (NativeMethods.int2dds_dynamic_value_element(h, i, out IntPtr o), o), index);
        public DynamicValue MapKey(int index) => Child((h, i) => (NativeMethods.int2dds_dynamic_value_map_key(h, i, out IntPtr o), o), index);
        public DynamicValue MapValue(int index) => Child((h, i) => (NativeMethods.int2dds_dynamic_value_map_value(h, i, out IntPtr o), o), index);

        /// <summary>Clone a struct value into a new <see cref="DynamicData"/> (caller disposes).</summary>
        public DynamicData AsStruct()
        {
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_value_as_struct(_handle, out IntPtr d));
            return new DynamicData(d);
        }

        public DynamicValue UnionDiscriminator() => Wrap(NativeMethods.int2dds_dynamic_value_union_discriminator(_handle, out IntPtr h), h);
        public DynamicValue UnionValue() => Wrap(NativeMethods.int2dds_dynamic_value_union_value(_handle, out IntPtr h), h);

        public void Dispose()
        {
            if (_handle != IntPtr.Zero)
            {
                NativeMethods.int2dds_dynamic_value_destroy(_handle);
                _handle = IntPtr.Zero;
            }
        }
    }
}
