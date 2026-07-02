using System;
using System.Runtime.InteropServices;
using Int2Dds.Core;
using Int2Dds.Exceptions;
using Int2Dds.Interop;

namespace Int2Dds.Xtypes
{
    /// <summary>
    /// XML configuration entry points, mirroring the Rust
    /// <c>DomainParticipantFactory</c> config APIs: load QoS profiles (and the
    /// <c>&lt;types&gt;</c> section) from XML, then build either a dynamic type
    /// support or an entire participant tree (<see cref="ConfiguredParticipant"/>)
    /// from a <c>&lt;domain_participant_library&gt;</c> declaration.
    /// </summary>
    public static class XmlConfig
    {
        /// <summary>
        /// Load QoS profiles (and, for XML files, the <c>&lt;types&gt;</c> section)
        /// from one or more files into the factory singleton. Loaded profiles are
        /// usable with the <c>*WithProfile</c> creators and with
        /// <see cref="CreateParticipantFromConfig"/>; types with
        /// <see cref="GetDynamicTypeSupport"/>.
        /// </summary>
        public static void LoadProfiles(params string[] paths)
        {
            if (paths == null || paths.Length == 0)
                throw new ArgumentException("at least one path is required", nameof(paths));

            var factory = DomainParticipantFactory.Instance;
            var ptrs = new IntPtr[paths.Length];
            try
            {
                for (int i = 0; i < paths.Length; i++)
                {
                    // UTF-8 (NUL-terminated) into unmanaged memory; TFM-agnostic
                    // (StringToCoTaskMemUTF8 is unavailable on net4x/netstandard2.0).
                    byte[] bytes = NativeString.ToCStr(paths[i]);
                    IntPtr mem = Marshal.AllocHGlobal(bytes.Length);
                    Marshal.Copy(bytes, 0, mem, bytes.Length);
                    ptrs[i] = mem;
                }
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_load_profiles(factory.Handle, ptrs, (UIntPtr)paths.Length));
            }
            finally
            {
                foreach (var p in ptrs)
                    if (p != IntPtr.Zero) Marshal.FreeHGlobal(p);
            }
        }

        /// <summary>
        /// Build a dynamic type support for a type declared in a <c>&lt;types&gt;</c>
        /// section loaded via <see cref="LoadProfiles"/>.
        /// </summary>
        public static unsafe DynamicTypeSupport GetDynamicTypeSupport(string typeName)
        {
            var factory = DomainParticipantFactory.Instance;
            var nb = NativeString.ToCStr(typeName);
            fixed (byte* p = nb)
            {
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_get_dynamic_type_support(factory.Handle, p, out IntPtr h));
                return new DynamicTypeSupport(h);
            }
        }

        /// <summary>
        /// Build an entire participant tree from a <c>&lt;domain_participant_library&gt;</c>
        /// declaration at <paramref name="path"/> (e.g. <c>"PL::PubApp"</c>). The XML
        /// must have been loaded via <see cref="LoadProfiles"/>.
        /// </summary>
        public static unsafe ConfiguredParticipant CreateParticipantFromConfig(string path)
        {
            var factory = DomainParticipantFactory.Instance;
            var pb = NativeString.ToCStr(path);
            fixed (byte* p = pb)
            {
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_create_participant_from_config(factory.Handle, p, out IntPtr h));
                return new ConfiguredParticipant(h);
            }
        }
    }
}
