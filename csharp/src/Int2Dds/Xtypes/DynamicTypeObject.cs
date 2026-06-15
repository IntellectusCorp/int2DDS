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
        public int Kind { get; }
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
