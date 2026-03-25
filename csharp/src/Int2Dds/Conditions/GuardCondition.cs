using System;
using Int2Dds.Exceptions;
using Int2Dds.Interop;

namespace Int2Dds.Conditions
{
    /// <summary>
    /// A user-controlled condition for signaling.
    /// GuardConditions can be attached to a WaitSet and manually triggered
    /// to wake up waiting threads.
    /// </summary>
    public sealed class GuardCondition : IDisposable
    {
        private IntPtr _handle;
        private bool _disposed;

        /// <summary>
        /// Creates a new GuardCondition.
        /// </summary>
        public GuardCondition()
        {
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_guard_condition_new(out _handle));
        }

        /// <summary>
        /// Gets the native handle for this guard condition.
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
        /// Gets or sets the trigger value.
        /// </summary>
        public bool TriggerValue
        {
            get
            {
                if (_disposed) throw new ObjectDisposedException(GetType().Name);
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_guard_condition_get_trigger_value(_handle, out bool value));
                return value;
            }
            set
            {
                if (_disposed) throw new ObjectDisposedException(GetType().Name);
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_guard_condition_set_trigger_value(_handle, value));
            }
        }

        /// <summary>
        /// Sets the trigger value to true.
        /// </summary>
        public void Trigger() => TriggerValue = true;

        /// <summary>
        /// Sets the trigger value to false.
        /// </summary>
        public void Reset() => TriggerValue = false;

        public void Dispose()
        {
            if (_disposed) return;
            _disposed = true;

            if (_handle != IntPtr.Zero)
            {
                NativeMethods.int2dds_guard_condition_delete(_handle);
                _handle = IntPtr.Zero;
            }
        }
    }
}
