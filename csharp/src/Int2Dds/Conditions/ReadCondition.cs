using System;
using Int2Dds.Exceptions;
using Int2Dds.Interop;

namespace Int2Dds.Conditions
{
    /// <summary>
    /// A condition that triggers on samples matching sample/view/instance state
    /// masks. Create it from a <see cref="Int2Dds.Core.DataReader{T}"/> and attach
    /// it to a <see cref="WaitSet"/>.
    /// </summary>
    public class ReadCondition : IDisposable
    {
        private IntPtr _handle;
        private bool _disposed;

        internal ReadCondition(IntPtr handle)
        {
            _handle = handle;
        }

        internal IntPtr Handle
        {
            get
            {
                if (_disposed) throw new ObjectDisposedException(GetType().Name);
                return _handle;
            }
        }

        /// <summary>
        /// Gets the current trigger value.
        /// </summary>
        public bool TriggerValue
        {
            get
            {
                if (_disposed) throw new ObjectDisposedException(GetType().Name);
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_readcondition_get_trigger_value(_handle, out bool value));
                return value;
            }
        }

        public void Dispose()
        {
            if (_disposed) return;
            _disposed = true;

            if (_handle != IntPtr.Zero)
            {
                NativeMethods.int2dds_readcondition_delete(_handle);
                _handle = IntPtr.Zero;
            }
        }
    }
}
