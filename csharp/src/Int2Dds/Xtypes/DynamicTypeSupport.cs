using System;
using Int2Dds.Exceptions;
using Int2Dds.Interop;

namespace Int2Dds.Xtypes
{
    /// <summary>
    /// A type's runtime support object: creates writable <see cref="DynamicData"/> instances
    /// and backs dynamic topics/endpoints. Obtain one from an <see cref="XmlTypeRegistry"/>.
    /// </summary>
    public sealed class DynamicTypeSupport : IDisposable
    {
        private IntPtr _handle;

        internal DynamicTypeSupport(IntPtr handle)
        {
            _handle = handle;
        }

        internal IntPtr Handle => _handle;

        /// <summary>Create an empty, writable <see cref="DynamicData"/> for this type.</summary>
        public DynamicData CreateData()
        {
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_data_create(_handle, out IntPtr d));
            return new DynamicData(d);
        }

        public void Dispose()
        {
            if (_handle != IntPtr.Zero)
            {
                NativeMethods.int2dds_dynamic_type_support_destroy(_handle);
                _handle = IntPtr.Zero;
            }
        }
    }
}
