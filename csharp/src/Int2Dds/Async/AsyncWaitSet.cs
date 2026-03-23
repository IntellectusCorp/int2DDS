using Int2Dds.Conditions;

namespace Int2Dds.Async;

/// <summary>
/// Async-compatible WaitSet wrapper.
/// Provides async/await interface for waiting on DDS conditions.
/// </summary>
public sealed class AsyncWaitSet : IAsyncDisposable, IDisposable
{
    private readonly WaitSet _waitSet;
    private bool _disposed;

    /// <summary>
    /// Creates a new AsyncWaitSet backed by a fresh <see cref="WaitSet"/>.
    /// </summary>
    public AsyncWaitSet()
    {
        _waitSet = new WaitSet();
    }

    /// <summary>
    /// Gets the underlying WaitSet.
    /// </summary>
    internal WaitSet Inner => _waitSet;

    /// <summary>
    /// Attaches a <see cref="GuardCondition"/> to the underlying WaitSet.
    /// </summary>
    public void Attach(GuardCondition condition) => _waitSet.Attach(condition);

    /// <summary>
    /// Detaches a <see cref="GuardCondition"/> from the underlying WaitSet.
    /// </summary>
    public void Detach(GuardCondition condition) => _waitSet.Detach(condition);

    /// <summary>
    /// Attaches a <see cref="StatusCondition"/> to the underlying WaitSet.
    /// </summary>
    public void Attach(StatusCondition condition) => _waitSet.Attach(condition);

    /// <summary>
    /// Detaches a <see cref="StatusCondition"/> from the underlying WaitSet.
    /// </summary>
    public void Detach(StatusCondition condition) => _waitSet.Detach(condition);

    /// <summary>
    /// Attaches a DataReader by its native handle.
    /// </summary>
    internal void AttachDataReader(nint readerHandle) => _waitSet.AttachDataReader(readerHandle);

    /// <summary>
    /// Detaches a DataReader by its native handle.
    /// </summary>
    internal void DetachDataReader(nint readerHandle) => _waitSet.DetachDataReader(readerHandle);

    /// <summary>
    /// Attaches a DataWriter by its native handle.
    /// </summary>
    internal void AttachDataWriter(nint writerHandle) => _waitSet.AttachDataWriter(writerHandle);

    /// <summary>
    /// Detaches a DataWriter by its native handle.
    /// </summary>
    internal void DetachDataWriter(nint writerHandle) => _waitSet.DetachDataWriter(writerHandle);

    /// <summary>
    /// Asynchronously waits for any attached condition to be triggered.
    /// The blocking wait is offloaded to the thread pool.
    /// </summary>
    /// <param name="timeout">Maximum time to wait. Pass <c>null</c> for infinite wait.</param>
    /// <param name="cancellationToken">Token to cancel the wait.</param>
    /// <returns>
    /// <c>true</c> if a condition was triggered; <c>false</c> if the timeout expired.
    /// </returns>
    public async Task<bool> WaitAsync(TimeSpan? timeout = null, CancellationToken cancellationToken = default)
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
        return await Task.Run(() => _waitSet.Wait(timeout), cancellationToken).ConfigureAwait(false);
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        _waitSet.Dispose();
    }

    public ValueTask DisposeAsync()
    {
        Dispose();
        return ValueTask.CompletedTask;
    }
}
