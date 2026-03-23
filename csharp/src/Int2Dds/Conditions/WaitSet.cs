using Int2Dds.Exceptions;
using Int2Dds.Interop;

namespace Int2Dds.Conditions;

/// <summary>
/// WaitSet — wait for multiple conditions to be triggered.
/// A WaitSet allows blocking until one or more attached conditions are triggered.
/// </summary>
public sealed class WaitSet : IDisposable
{
    private nint _handle;
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
    internal nint Handle
    {
        get
        {
            ObjectDisposedException.ThrowIf(_disposed, this);
            return _handle;
        }
    }

    /// <summary>
    /// Attaches a <see cref="GuardCondition"/> to this WaitSet.
    /// </summary>
    public void Attach(GuardCondition condition)
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
        ArgumentNullException.ThrowIfNull(condition);
        ReturnCodeHelper.CheckReturn(
            NativeMethods.int2dds_waitset_attach_guard_condition(_handle, condition.Handle));
    }

    /// <summary>
    /// Detaches a <see cref="GuardCondition"/> from this WaitSet.
    /// </summary>
    public void Detach(GuardCondition condition)
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
        ArgumentNullException.ThrowIfNull(condition);
        ReturnCodeHelper.CheckReturn(
            NativeMethods.int2dds_waitset_detach_guard_condition(_handle, condition.Handle));
    }

    /// <summary>
    /// Attaches a <see cref="StatusCondition"/> to this WaitSet.
    /// </summary>
    public void Attach(StatusCondition condition)
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
        ArgumentNullException.ThrowIfNull(condition);
        ReturnCodeHelper.CheckReturn(
            NativeMethods.int2dds_waitset_attach_condition(_handle, condition.Handle));
    }

    /// <summary>
    /// Detaches a <see cref="StatusCondition"/> from this WaitSet.
    /// </summary>
    public void Detach(StatusCondition condition)
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
        ArgumentNullException.ThrowIfNull(condition);
        ReturnCodeHelper.CheckReturn(
            NativeMethods.int2dds_waitset_detach_condition(_handle, condition.Handle));
    }

    /// <summary>
    /// Attaches a DataReader by its native handle.
    /// </summary>
    internal void AttachDataReader(nint readerHandle)
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
        ReturnCodeHelper.CheckReturn(
            NativeMethods.int2dds_waitset_attach_datareader(_handle, readerHandle));
    }

    /// <summary>
    /// Detaches a DataReader by its native handle.
    /// </summary>
    internal void DetachDataReader(nint readerHandle)
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
        ReturnCodeHelper.CheckReturn(
            NativeMethods.int2dds_waitset_detach_datareader(_handle, readerHandle));
    }

    /// <summary>
    /// Attaches a DataWriter by its native handle.
    /// </summary>
    internal void AttachDataWriter(nint writerHandle)
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
        ReturnCodeHelper.CheckReturn(
            NativeMethods.int2dds_waitset_attach_datawriter(_handle, writerHandle));
    }

    /// <summary>
    /// Detaches a DataWriter by its native handle.
    /// </summary>
    internal void DetachDataWriter(nint writerHandle)
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
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
        ObjectDisposedException.ThrowIf(_disposed, this);

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
        ObjectDisposedException.ThrowIf(_disposed, this);

        long timeoutMs = timeout.HasValue
            ? (long)timeout.Value.TotalMilliseconds
            : -1;

        int ret = NativeMethods.int2dds_waitset_wait_ex(_handle, timeoutMs, out nint seqHandle);

        if (ret == ReturnCode.Timeout)
            return Array.Empty<Condition>();

        ReturnCodeHelper.CheckReturn(ret);

        try
        {
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_condition_seq_length(seqHandle, out nuint count));

            var conditions = new Condition[(int)count];
            for (nuint i = 0; i < count; i++)
            {
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_condition_seq_get(seqHandle, i, out nint condHandle));
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

        if (_handle != 0)
        {
            NativeMethods.int2dds_waitset_delete(_handle);
            _handle = 0;
        }
    }
}
