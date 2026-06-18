using System;
using System.Text;
using Int2Dds.Exceptions;
using Int2Dds.Interop;

namespace Int2Dds.Xtypes
{
    /// <summary>
    /// A decoded sample. Read fields by dotted/indexed path (e.g. "pos.x", "tags[2]").
    /// </summary>
    public sealed class DynamicData : IDisposable
    {
        private IntPtr _handle;

        internal DynamicData(IntPtr handle)
        {
            _handle = handle;
        }

        internal IntPtr Handle => _handle;

        public unsafe bool GetBool(string path)
        {
            var pb = Encoding.UTF8.GetBytes(path + '\0');
            fixed (byte* p = pb)
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_data_get_bool(_handle, p, out byte v));
                return v != 0;
            }
        }

        public unsafe sbyte GetI8(string path)
        {
            var pb = Encoding.UTF8.GetBytes(path + '\0');
            fixed (byte* p = pb)
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_data_get_i8(_handle, p, out sbyte v));
                return v;
            }
        }

        public unsafe byte GetU8(string path)
        {
            var pb = Encoding.UTF8.GetBytes(path + '\0');
            fixed (byte* p = pb)
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_data_get_u8(_handle, p, out byte v));
                return v;
            }
        }

        public unsafe short GetI16(string path)
        {
            var pb = Encoding.UTF8.GetBytes(path + '\0');
            fixed (byte* p = pb)
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_data_get_i16(_handle, p, out short v));
                return v;
            }
        }

        public unsafe ushort GetU16(string path)
        {
            var pb = Encoding.UTF8.GetBytes(path + '\0');
            fixed (byte* p = pb)
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_data_get_u16(_handle, p, out ushort v));
                return v;
            }
        }

        public unsafe int GetI32(string path)
        {
            var pb = Encoding.UTF8.GetBytes(path + '\0');
            fixed (byte* p = pb)
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_data_get_i32(_handle, p, out int v));
                return v;
            }
        }

        public unsafe uint GetU32(string path)
        {
            var pb = Encoding.UTF8.GetBytes(path + '\0');
            fixed (byte* p = pb)
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_data_get_u32(_handle, p, out uint v));
                return v;
            }
        }

        public unsafe long GetI64(string path)
        {
            var pb = Encoding.UTF8.GetBytes(path + '\0');
            fixed (byte* p = pb)
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_data_get_i64(_handle, p, out long v));
                return v;
            }
        }

        public unsafe ulong GetU64(string path)
        {
            var pb = Encoding.UTF8.GetBytes(path + '\0');
            fixed (byte* p = pb)
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_data_get_u64(_handle, p, out ulong v));
                return v;
            }
        }

        public unsafe float GetF32(string path)
        {
            var pb = Encoding.UTF8.GetBytes(path + '\0');
            fixed (byte* p = pb)
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_data_get_f32(_handle, p, out float v));
                return v;
            }
        }

        public unsafe double GetF64(string path)
        {
            var pb = Encoding.UTF8.GetBytes(path + '\0');
            fixed (byte* p = pb)
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_data_get_f64(_handle, p, out double v));
                return v;
            }
        }

        public unsafe byte GetChar8(string path)
        {
            var pb = Encoding.UTF8.GetBytes(path + '\0');
            fixed (byte* p = pb)
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_data_get_char8(_handle, p, out byte v));
                return v;
            }
        }

        public unsafe string GetString(string path)
        {
            var pb = Encoding.UTF8.GetBytes(path + '\0');
            byte[] buf = new byte[256];
            UIntPtr outLen;
            fixed (byte* pPath = pb)
            fixed (byte* pBuf = buf)
            {
                int ret = NativeMethods.int2dds_dynamic_data_get_string(_handle, pPath, pBuf, (UIntPtr)buf.Length, out outLen);
                if (ret == 0)
                    return Encoding.UTF8.GetString(buf, 0, (int)outLen);
            }
            int need = (int)outLen;
            buf = new byte[need + 1];
            fixed (byte* pPath = pb)
            fixed (byte* pBuf = buf)
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_data_get_string(_handle, pPath, pBuf, (UIntPtr)buf.Length, out outLen));
                return Encoding.UTF8.GetString(buf, 0, (int)outLen);
            }
        }

        /// <summary>Element count of a sequence/array field.</summary>
        public unsafe int GetLength(string path)
        {
            var pb = Encoding.UTF8.GetBytes(path + '\0');
            fixed (byte* p = pb)
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_data_get_len(_handle, p, out UIntPtr len));
                return (int)len;
            }
        }

        /// <summary>Extract a nested struct field as a new DynamicData (caller disposes).</summary>
        public unsafe DynamicData GetMember(string path)
        {
            var pb = Encoding.UTF8.GetBytes(path + '\0');
            fixed (byte* p = pb)
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_data_get_member(_handle, p, out IntPtr child));
                return new DynamicData(child);
            }
        }

        // --- Writable setters (build a sample to publish) ---

        public unsafe DynamicData SetBool(string field, bool value)
        {
            var f = NativeString.ToCStr(field);
            fixed (byte* p = f)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_data_set_bool(_handle, p, (byte)(value ? 1 : 0)));
            return this;
        }

        public unsafe DynamicData SetI8(string field, sbyte value)
        {
            var f = NativeString.ToCStr(field);
            fixed (byte* p = f)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_data_set_i8(_handle, p, value));
            return this;
        }

        public unsafe DynamicData SetU8(string field, byte value)
        {
            var f = NativeString.ToCStr(field);
            fixed (byte* p = f)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_data_set_u8(_handle, p, value));
            return this;
        }

        public unsafe DynamicData SetI16(string field, short value)
        {
            var f = NativeString.ToCStr(field);
            fixed (byte* p = f)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_data_set_i16(_handle, p, value));
            return this;
        }

        public unsafe DynamicData SetU16(string field, ushort value)
        {
            var f = NativeString.ToCStr(field);
            fixed (byte* p = f)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_data_set_u16(_handle, p, value));
            return this;
        }

        public unsafe DynamicData SetI32(string field, int value)
        {
            var f = NativeString.ToCStr(field);
            fixed (byte* p = f)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_data_set_i32(_handle, p, value));
            return this;
        }

        public unsafe DynamicData SetU32(string field, uint value)
        {
            var f = NativeString.ToCStr(field);
            fixed (byte* p = f)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_data_set_u32(_handle, p, value));
            return this;
        }

        public unsafe DynamicData SetI64(string field, long value)
        {
            var f = NativeString.ToCStr(field);
            fixed (byte* p = f)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_data_set_i64(_handle, p, value));
            return this;
        }

        public unsafe DynamicData SetU64(string field, ulong value)
        {
            var f = NativeString.ToCStr(field);
            fixed (byte* p = f)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_data_set_u64(_handle, p, value));
            return this;
        }

        public unsafe DynamicData SetF32(string field, float value)
        {
            var f = NativeString.ToCStr(field);
            fixed (byte* p = f)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_data_set_f32(_handle, p, value));
            return this;
        }

        public unsafe DynamicData SetF64(string field, double value)
        {
            var f = NativeString.ToCStr(field);
            fixed (byte* p = f)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_data_set_f64(_handle, p, value));
            return this;
        }

        public unsafe DynamicData SetChar8(string field, byte value)
        {
            var f = NativeString.ToCStr(field);
            fixed (byte* p = f)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_data_set_char8(_handle, p, value));
            return this;
        }

        public unsafe DynamicData SetString(string field, string value)
        {
            var f = NativeString.ToCStr(field);
            var v = NativeString.ToCStr(value);
            fixed (byte* pf = f)
            fixed (byte* pv = v)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_data_set_string(_handle, pf, pv));
            return this;
        }

        /// <summary>
        /// Set <paramref name="field"/> from a (possibly nested) <see cref="DynamicValue"/> tree.
        /// The value is consumed on success and must not be reused.
        /// </summary>
        public unsafe DynamicData SetValue(string field, DynamicValue value)
        {
            if (value == null) throw new ArgumentNullException(nameof(value));
            var f = NativeString.ToCStr(field);
            int ret;
            fixed (byte* p = f)
                ret = NativeMethods.int2dds_dynamic_data_set_value(_handle, p, value.Handle);
            if (ret != ReturnCode.NullPointer && ret != ReturnCode.InvalidArgument)
                value.Consume();
            ReturnCodeHelper.CheckReturn(ret);
            return this;
        }

        /// <summary>Clone the value at a dotted/indexed path into a new <see cref="DynamicValue"/> tree.</summary>
        public unsafe DynamicValue GetValue(string path)
        {
            var pb = NativeString.ToCStr(path);
            fixed (byte* p = pb)
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_data_get_value(_handle, p, out IntPtr v));
                return new DynamicValue(v);
            }
        }

        public void Dispose()
        {
            if (_handle != IntPtr.Zero)
            {
                NativeMethods.int2dds_dynamic_data_destroy(_handle);
                _handle = IntPtr.Zero;
            }
        }
    }
}
