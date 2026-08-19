using System;
using System.Collections.Generic;
using System.Text;
using Int2Dds.Exceptions;
using Int2Dds.Interop;

namespace Int2Dds.Xtypes
{
    /// <summary>Per-member metadata from TypeObject introspection.</summary>
    public readonly struct MemberInfo
    {
        public string Name { get; }
        public uint MemberId { get; }

        /// <summary>One of <see cref="Int2Dds.Types.FieldType"/>.</summary>
        public int Kind { get; }

        /// <summary>Bitmask of <see cref="Int2Dds.Types.MemberFlags"/>.</summary>
        public int Flags { get; }

        public MemberInfo(string name, uint memberId, int kind, int flags)
        {
            Name = name;
            MemberId = memberId;
            Kind = kind;
            Flags = flags;
        }
    }

    /// <summary>
    /// A discovered or locally-built TypeObject; supports struct member introspection.
    /// </summary>
    public sealed class DynamicTypeObject : IDisposable
    {
        private IntPtr _handle;

        internal DynamicTypeObject(IntPtr handle)
        {
            _handle = handle;
        }

        internal IntPtr Handle => _handle;

        /// <summary>Extensibility: 0 = Final, 1 = Appendable, 2 = Mutable.</summary>
        public int Extensibility
        {
            get
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_type_object_extensibility(_handle, out int kind));
                return kind;
            }
        }

        public uint MemberCount
        {
            get
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_type_object_member_count(_handle, out uint count));
                return count;
            }
        }

        public unsafe MemberInfo GetMember(uint index)
        {
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_type_object_member_info(_handle, index, out Int2DdsMemberInfo info));
            byte[] buf = new byte[256];
            UIntPtr outLen;
            fixed (byte* p = buf)
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_type_object_member_name(_handle, index, p, (UIntPtr)buf.Length, out outLen));
            }
            string name = Encoding.UTF8.GetString(buf, 0, (int)outLen);
            return new MemberInfo(name, info.MemberId, info.Kind, info.Flags);
        }

        public IReadOnlyList<MemberInfo> Members()
        {
            uint count = MemberCount;
            var list = new List<MemberInfo>((int)count);
            for (uint i = 0; i < count; i++)
                list.Add(GetMember(i));
            return list;
        }

        public unsafe uint FindMember(string name)
        {
            var nameBytes = Encoding.UTF8.GetBytes(name + '\0');
            fixed (byte* p = nameBytes)
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_type_object_find_member(_handle, p, out uint index));
                return index;
            }
        }

        /// <summary>Reads a bool field directly from raw CDR bytes using this TypeObject.</summary>
        public unsafe bool GetSampleBool(byte[] bytes, string fieldName)
        {
            fixed (byte* pBytes = bytes)
            fixed (byte* pName = NameBytes(fieldName))
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_sample_get_bool(pBytes, (UIntPtr)bytes.Length, _handle, pName, out bool v));
                return v;
            }
        }

        public unsafe sbyte GetSampleI8(byte[] bytes, string fieldName)
        {
            fixed (byte* pBytes = bytes)
            fixed (byte* pName = NameBytes(fieldName))
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_sample_get_i8(pBytes, (UIntPtr)bytes.Length, _handle, pName, out sbyte v));
                return v;
            }
        }

        public unsafe byte GetSampleU8(byte[] bytes, string fieldName)
        {
            fixed (byte* pBytes = bytes)
            fixed (byte* pName = NameBytes(fieldName))
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_sample_get_u8(pBytes, (UIntPtr)bytes.Length, _handle, pName, out byte v));
                return v;
            }
        }

        public unsafe byte GetSampleByte(byte[] bytes, string fieldName)
        {
            fixed (byte* pBytes = bytes)
            fixed (byte* pName = NameBytes(fieldName))
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_sample_get_byte(pBytes, (UIntPtr)bytes.Length, _handle, pName, out byte v));
                return v;
            }
        }

        public unsafe byte GetSampleChar8(byte[] bytes, string fieldName)
        {
            fixed (byte* pBytes = bytes)
            fixed (byte* pName = NameBytes(fieldName))
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_sample_get_char8(pBytes, (UIntPtr)bytes.Length, _handle, pName, out byte v));
                return v;
            }
        }

        public unsafe short GetSampleI16(byte[] bytes, string fieldName)
        {
            fixed (byte* pBytes = bytes)
            fixed (byte* pName = NameBytes(fieldName))
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_sample_get_i16(pBytes, (UIntPtr)bytes.Length, _handle, pName, out short v));
                return v;
            }
        }

        public unsafe ushort GetSampleU16(byte[] bytes, string fieldName)
        {
            fixed (byte* pBytes = bytes)
            fixed (byte* pName = NameBytes(fieldName))
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_sample_get_u16(pBytes, (UIntPtr)bytes.Length, _handle, pName, out ushort v));
                return v;
            }
        }

        public unsafe int GetSampleI32(byte[] bytes, string fieldName)
        {
            fixed (byte* pBytes = bytes)
            fixed (byte* pName = NameBytes(fieldName))
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_sample_get_i32(pBytes, (UIntPtr)bytes.Length, _handle, pName, out int v));
                return v;
            }
        }

        public unsafe uint GetSampleU32(byte[] bytes, string fieldName)
        {
            fixed (byte* pBytes = bytes)
            fixed (byte* pName = NameBytes(fieldName))
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_sample_get_u32(pBytes, (UIntPtr)bytes.Length, _handle, pName, out uint v));
                return v;
            }
        }

        public unsafe long GetSampleI64(byte[] bytes, string fieldName)
        {
            fixed (byte* pBytes = bytes)
            fixed (byte* pName = NameBytes(fieldName))
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_sample_get_i64(pBytes, (UIntPtr)bytes.Length, _handle, pName, out long v));
                return v;
            }
        }

        public unsafe ulong GetSampleU64(byte[] bytes, string fieldName)
        {
            fixed (byte* pBytes = bytes)
            fixed (byte* pName = NameBytes(fieldName))
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_sample_get_u64(pBytes, (UIntPtr)bytes.Length, _handle, pName, out ulong v));
                return v;
            }
        }

        public unsafe float GetSampleF32(byte[] bytes, string fieldName)
        {
            fixed (byte* pBytes = bytes)
            fixed (byte* pName = NameBytes(fieldName))
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_sample_get_f32(pBytes, (UIntPtr)bytes.Length, _handle, pName, out float v));
                return v;
            }
        }

        public unsafe double GetSampleF64(byte[] bytes, string fieldName)
        {
            fixed (byte* pBytes = bytes)
            fixed (byte* pName = NameBytes(fieldName))
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_sample_get_f64(pBytes, (UIntPtr)bytes.Length, _handle, pName, out double v));
                return v;
            }
        }

        /// <summary>Reads a string field directly from raw CDR bytes using this TypeObject.</summary>
        public unsafe string GetSampleString(byte[] bytes, string fieldName)
        {
            var outBuf = new byte[256];
            fixed (byte* pBytes = bytes)
            fixed (byte* pName = NameBytes(fieldName))
            fixed (byte* pOut = outBuf)
            {
                UIntPtr outLen;
                int ret = NativeMethods.int2dds_dynamic_sample_get_string(pBytes, (UIntPtr)bytes.Length, _handle, pName, pOut, (UIntPtr)outBuf.Length, out outLen);
                // The native copy needs one extra byte for the NUL terminator, so the
                // required capacity is outLen + 1. Retry whenever that exceeds the buffer.
                if (ret == ReturnCode.BufferTooSmall || (int)(ulong)outLen + 1 > outBuf.Length)
                {
                    outBuf = new byte[(int)(ulong)outLen + 1];
                    fixed (byte* pOut2 = outBuf)
                    {
                        ret = NativeMethods.int2dds_dynamic_sample_get_string(pBytes, (UIntPtr)bytes.Length, _handle, pName, pOut2, (UIntPtr)outBuf.Length, out outLen);
                    }
                }
                ReturnCodeHelper.CheckReturn(ret);
                int len = (int)(ulong)outLen;
                if (len > 0 && outBuf[len - 1] == 0) len--;
                return Encoding.UTF8.GetString(outBuf, 0, len);
            }
        }

        private static byte[] NameBytes(string fieldName) => Encoding.UTF8.GetBytes(fieldName + '\0');

        public void Dispose()
        {
            if (_handle != IntPtr.Zero)
            {
                NativeMethods.int2dds_type_object_destroy(_handle);
                _handle = IntPtr.Zero;
            }
        }
    }
}
