using System;
using System.Collections.Generic;
using Int2Dds.Exceptions;
using Int2Dds.Interop;

namespace Int2Dds.Xtypes
{
    /// <summary>
    /// Loads DDS types described in XML at runtime and
    /// hands out <see cref="DynamicTypeSupport"/> / <see cref="DynamicTypeObject"/> for them —
    /// no compile-time IDL required.
    /// </summary>
    public sealed class XmlTypeRegistry : IDisposable
    {
        private IntPtr _handle;

        /// <summary>Create an empty registry.</summary>
        public XmlTypeRegistry()
        {
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_xml_type_registry_create(out _handle));
        }

        private XmlTypeRegistry(IntPtr handle)
        {
            _handle = handle;
        }

        /// <summary>Create a registry and load <paramref name="path"/> into it in one step.</summary>
        public static unsafe XmlTypeRegistry FromFile(string path)
        {
            var pb = NativeString.ToCStr(path);
            fixed (byte* p = pb)
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_xml_type_registry_from_file(p, out IntPtr h));
                return new XmlTypeRegistry(h);
            }
        }

        /// <summary>Load additional types from an XML file.</summary>
        public unsafe XmlTypeRegistry LoadFile(string path)
        {
            var pb = NativeString.ToCStr(path);
            fixed (byte* p = pb)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_xml_type_registry_load_file(_handle, p));
            return this;
        }

        /// <summary>Load additional types from an in-memory XML string.</summary>
        public unsafe XmlTypeRegistry LoadString(string xml)
        {
            var xb = NativeString.ToCStr(xml);
            fixed (byte* p = xb)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_xml_type_registry_load_str(_handle, p));
            return this;
        }

        /// <summary>Look up a loaded type by name and build its <see cref="DynamicTypeSupport"/>.</summary>
        public unsafe DynamicTypeSupport GetTypeSupport(string name)
        {
            var nb = NativeString.ToCStr(name);
            fixed (byte* p = nb)
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_xml_type_registry_get_type_support(_handle, p, out IntPtr h));
                return new DynamicTypeSupport(h);
            }
        }

        /// <summary>Look up a loaded type by name and return its <see cref="DynamicTypeObject"/>.</summary>
        public unsafe DynamicTypeObject GetTypeObject(string name)
        {
            var nb = NativeString.ToCStr(name);
            fixed (byte* p = nb)
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_xml_type_registry_get_type_object(_handle, p, out IntPtr h));
                return new DynamicTypeObject(h);
            }
        }

        /// <summary>Number of types loaded into the registry.</summary>
        public int TypeCount()
        {
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_xml_type_registry_type_count(_handle, out UIntPtr count));
            return (int)count;
        }

        /// <summary>Fully-qualified name of the type at <paramref name="index"/>.</summary>
        public unsafe string TypeName(int index)
        {
            IntPtr h = _handle;
            return NativeString.Read((byte* buf, UIntPtr cap, out UIntPtr len) =>
                NativeMethods.int2dds_xml_type_registry_type_name(h, (UIntPtr)index, buf, cap, out len));
        }

        /// <summary>Fully-qualified names of all loaded types.</summary>
        public IReadOnlyList<string> TypeNames()
        {
            int count = TypeCount();
            var names = new List<string>(count);
            for (int i = 0; i < count; i++)
                names.Add(TypeName(i));
            return names;
        }

        public void Dispose()
        {
            if (_handle != IntPtr.Zero)
            {
                NativeMethods.int2dds_xml_type_registry_destroy(_handle);
                _handle = IntPtr.Zero;
            }
        }
    }
}
