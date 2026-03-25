using System;
using Int2Dds.Exceptions;
using Int2Dds.Interop;

namespace Int2Dds.Conditions
{
    /// <summary>
    /// A read-only condition returned from <see cref="WaitSet.WaitEx"/>.
    /// Represents a triggered condition from the WaitSet.
    /// </summary>
    public sealed class Condition : IDisposable
    {
        private IntPtr _handle;
        private bool _disposed;

        internal Condition(IntPtr handle)
        {
            _handle = handle;
        }

        /// <summary>
        /// Gets the native handle for this condition.
        /// </summary>
        internal IntPtr Handle
        {
            get
            {
                if (_disposed) throw new ObjectDisposedException(GetType().Name);
                return _handle;
            }
        }

        /// <summary>
        /// Gets the current trigger value of this condition.
        /// </summary>
        public bool TriggerValue
        {
            get
            {
                if (_disposed) throw new ObjectDisposedException(GetType().Name);
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_condition_get_trigger_value(_handle, out bool triggered));
                return triggered;
            }
        }

        public void Dispose()
        {
            if (_disposed) return;
            _disposed = true;

            if (_handle != IntPtr.Zero)
            {
                NativeMethods.int2dds_condition_delete(_handle);
                _handle = IntPtr.Zero;
            }
        }
    }
}
