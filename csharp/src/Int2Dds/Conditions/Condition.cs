using Int2Dds.Exceptions;
using Int2Dds.Interop;

namespace Int2Dds.Conditions;

/// <summary>
/// A read-only condition returned from <see cref="WaitSet.WaitEx"/>.
/// Represents a triggered condition from the WaitSet.
/// </summary>
public sealed class Condition : IDisposable
{
    private nint _handle;
    private bool _disposed;

    internal Condition(nint handle)
    {
        _handle = handle;
    }

    /// <summary>
    /// Gets the native handle for this condition.
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
    /// Gets the current trigger value of this condition.
    /// </summary>
    public bool TriggerValue
    {
        get
        {
            ObjectDisposedException.ThrowIf(_disposed, this);
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_condition_get_trigger_value(_handle, out bool triggered));
            return triggered;
        }
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;

        if (_handle != 0)
        {
            NativeMethods.int2dds_condition_delete(_handle);
            _handle = 0;
        }
    }
}
