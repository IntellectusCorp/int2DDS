using System;
using Int2Dds.Exceptions;
using Int2Dds.Interop;

namespace Int2Dds.Conditions
{
    /// <summary>
    /// A condition based on entity status changes.
    /// StatusConditions are obtained from DataReaders or DataWriters
    /// and can be attached to a WaitSet.
    /// </summary>
    public sealed class StatusCondition : IDisposable
    {
        private IntPtr _handle;
        private bool _disposed;

        /// <summary>
        /// Creates a StatusCondition wrapping an existing FFI handle.
        /// </summary>
        internal StatusCondition(IntPtr handle)
        {
            _handle = handle;
        }

        /// <summary>
        /// Gets the native handle for this status condition.
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
        /// Gets or sets the enabled status mask.
        /// </summary>
        public StatusMask EnabledStatuses
        {
            get
            {
                if (_disposed) throw new ObjectDisposedException(GetType().Name);
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_statuscondition_get_enabled_statuses(_handle, out uint mask));
                return (StatusMask)mask;
            }
            set
            {
                if (_disposed) throw new ObjectDisposedException(GetType().Name);
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_statuscondition_set_enabled_statuses(_handle, (uint)value));
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
                    NativeMethods.int2dds_statuscondition_get_trigger_value(_handle, out bool value));
                return value;
            }
        }

        public void Dispose()
        {
            if (_disposed) return;
            _disposed = true;

            if (_handle != IntPtr.Zero)
            {
                NativeMethods.int2dds_statuscondition_delete(_handle);
                _handle = IntPtr.Zero;
            }
        }
    }
}
