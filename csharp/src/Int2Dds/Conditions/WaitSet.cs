using System;
using System.Collections.Generic;
using Int2Dds.Exceptions;
using Int2Dds.Interop;

namespace Int2Dds.Conditions
{
    /// <summary>
    /// WaitSet — wait for multiple conditions to be triggered.
    /// A WaitSet allows blocking until one or more attached conditions are triggered.
    /// </summary>
    public sealed class WaitSet : IDisposable
    {
        private IntPtr _handle;
        private bool _disposed;

        /// <summary>
        /// Creates a new WaitSet.
        /// </summary>
        public WaitSet()
        {
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_waitset_new(out _handle));
        }

        /// <summary>
        /// Gets the native handle.
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
        /// Attaches a <see cref="GuardCondition"/> to this WaitSet.
        /// </summary>
        public void Attach(GuardCondition condition)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            if (condition == null) throw new ArgumentNullException(nameof(condition));
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_waitset_attach_guard_condition(_handle, condition.Handle));
        }

        /// <summary>
        /// Detaches a <see cref="GuardCondition"/> from this WaitSet.
        /// </summary>
        public void Detach(GuardCondition condition)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            if (condition == null) throw new ArgumentNullException(nameof(condition));
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_waitset_detach_guard_condition(_handle, condition.Handle));
        }

        /// <summary>
        /// Attaches a <see cref="StatusCondition"/> to this WaitSet.
        /// </summary>
        public void Attach(StatusCondition condition)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            if (condition == null) throw new ArgumentNullException(nameof(condition));
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_waitset_attach_condition(_handle, condition.Handle));
        }

        /// <summary>
        /// Detaches a <see cref="StatusCondition"/> from this WaitSet.
        /// </summary>
        public void Detach(StatusCondition condition)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            if (condition == null) throw new ArgumentNullException(nameof(condition));
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_waitset_detach_condition(_handle, condition.Handle));
        }

        /// <summary>
        /// Attaches a <see cref="ReadCondition"/> (or <see cref="QueryCondition"/>) to this WaitSet.
        /// </summary>
        public void Attach(ReadCondition condition)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            if (condition == null) throw new ArgumentNullException(nameof(condition));
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_waitset_attach_readcondition(_handle, condition.Handle));
        }

        /// <summary>
        /// Detaches a <see cref="ReadCondition"/> (or <see cref="QueryCondition"/>) from this WaitSet.
        /// </summary>
        public void Detach(ReadCondition condition)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            if (condition == null) throw new ArgumentNullException(nameof(condition));
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_waitset_detach_readcondition(_handle, condition.Handle));
        }

        /// <summary>
        /// Attaches a DataReader by its native handle.
        /// </summary>
        internal void AttachDataReader(IntPtr readerHandle)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_waitset_attach_datareader(_handle, readerHandle));
        }

        /// <summary>
        /// Detaches a DataReader by its native handle.
        /// </summary>
        internal void DetachDataReader(IntPtr readerHandle)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_waitset_detach_datareader(_handle, readerHandle));
        }

        /// <summary>
        /// Attaches a DataWriter by its native handle.
        /// </summary>
        internal void AttachDataWriter(IntPtr writerHandle)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_waitset_attach_datawriter(_handle, writerHandle));
        }

        /// <summary>
        /// Detaches a DataWriter by its native handle.
        /// </summary>
        internal void DetachDataWriter(IntPtr writerHandle)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_waitset_detach_datawriter(_handle, writerHandle));
        }

        /// <summary>
        /// Waits for any attached condition to be triggered.
        /// </summary>
        /// <param name="timeout">
        /// Maximum time to wait. Pass <c>null</c> for infinite wait.
        /// </param>
        /// <returns>
        /// <c>true</c> if a condition was triggered; <c>false</c> if the timeout expired.
        /// </returns>
        public bool Wait(TimeSpan? timeout = null)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

            long timeoutMs = timeout.HasValue
                ? (long)timeout.Value.TotalMilliseconds
                : -1;

            int ret = NativeMethods.int2dds_waitset_wait(_handle, timeoutMs);

            if (ret == ReturnCode.Timeout)
                return false;

            ReturnCodeHelper.CheckReturn(ret);
            return true;
        }

        /// <summary>
        /// Waits for conditions and returns the list of triggered conditions.
        /// </summary>
        /// <param name="timeout">
        /// Maximum time to wait. Pass <c>null</c> for infinite wait.
        /// </param>
        /// <returns>
        /// A read-only list of triggered <see cref="Condition"/> objects.
        /// Returns an empty list if the timeout expired.
        /// </returns>
        public IReadOnlyList<Condition> WaitEx(TimeSpan? timeout = null)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

            long timeoutMs = timeout.HasValue
                ? (long)timeout.Value.TotalMilliseconds
                : -1;

            int ret = NativeMethods.int2dds_waitset_wait_ex(_handle, timeoutMs, out IntPtr seqHandle);
            return CollectConditions(ret, seqHandle);
        }

        /// <summary>
        /// Nanosecond-precision variant of <see cref="WaitEx"/>. Pass a negative
        /// value for an infinite wait.
        /// </summary>
        public IReadOnlyList<Condition> WaitExNs(long timeoutNs)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

            int ret = NativeMethods.int2dds_waitset_wait_ex_ns(_handle, timeoutNs, out IntPtr seqHandle);
            return CollectConditions(ret, seqHandle);
        }

        private IReadOnlyList<Condition> CollectConditions(int ret, IntPtr seqHandle)
        {
            if (ret == ReturnCode.Timeout)
                return Int2Dds.Internal.EmptyArrayHolder<Condition>.Value;

            ReturnCodeHelper.CheckReturn(ret);

            try
            {
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_condition_seq_length(seqHandle, out UIntPtr count));

                var conditions = new Condition[(int)(uint)count];
                for (uint i = 0; i < (uint)count; i++)
                {
                    ReturnCodeHelper.CheckReturn(
                        NativeMethods.int2dds_condition_seq_get(seqHandle, (UIntPtr)i, out IntPtr condHandle));
                    conditions[(int)i] = new Condition(condHandle);
                }

                return conditions;
            }
            finally
            {
                NativeMethods.int2dds_condition_seq_delete(seqHandle);
            }
        }

        public void Dispose()
        {
            if (_disposed) return;
            _disposed = true;

            if (_handle != IntPtr.Zero)
            {
                NativeMethods.int2dds_waitset_delete(_handle);
                _handle = IntPtr.Zero;
            }
        }
    }
}
