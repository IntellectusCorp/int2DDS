using System;
using System.Collections.Generic;
using System.Runtime.CompilerServices;
using System.Threading;
using System.Threading.Tasks;
using Int2Dds.Conditions;
using Int2Dds.Core;

namespace Int2Dds.Async
{
    /// <summary>
    /// Represents a sample with data and metadata returned by a DataReader.
    /// </summary>
    /// <typeparam name="T">The DDS type.</typeparam>
    public readonly struct Sample<T>
    {
        /// <summary>The deserialized data payload (may be default if <see cref="ValidData"/> is false).</summary>
        public T Data { get; }

        /// <summary>Whether the sample contains valid data.</summary>
        public bool ValidData { get; }

        public Sample(T data, bool validData)
        {
            Data = data;
            ValidData = validData;
        }
    }

    /// <summary>
    /// Interface for a DataReader that supports take/read operations.
    /// This allows AsyncDataReader to work without a direct dependency on a concrete DataReader class.
    /// </summary>
    /// <typeparam name="T">The DDS type.</typeparam>
    public interface IDataReader<T>
    {
        /// <summary>Takes all available samples, removing them from the reader cache.</summary>
        IReadOnlyList<Sample<T>> Take();

        /// <summary>Reads all available samples without removing them from the reader cache.</summary>
        IReadOnlyList<Sample<T>> Read();

        /// <summary>Gets the native handle for this DataReader.</summary>
        IntPtr Handle { get; }
    }

    /// <summary>
    /// Async-compatible DataReader wrapper.
    /// Provides async/await and IAsyncEnumerable interface for reading DDS samples.
    /// </summary>
    /// <typeparam name="T">The DDS type.</typeparam>
    public sealed class AsyncDataReader<T> : IAsyncDisposable, IDisposable
    {
        private readonly IDataReader<T> _reader;
        private AsyncWaitSet _waitSet;
        private bool _disposed;

        /// <summary>
        /// Creates an async wrapper for a DataReader.
        /// </summary>
        /// <param name="reader">The underlying DataReader.</param>
        public AsyncDataReader(IDataReader<T> reader)
        {
            if (reader == null) throw new ArgumentNullException(nameof(reader));
            _reader = reader;
        }

        /// <summary>
        /// Gets the underlying DataReader.
        /// </summary>
        public IDataReader<T> Reader => _reader;

        /// <summary>
        /// Asynchronously takes all available samples, removing them from the reader cache.
        /// </summary>
        public Task<IReadOnlyList<Sample<T>>> TakeAsync(CancellationToken cancellationToken = default)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            return Task.Run(() => _reader.Take(), cancellationToken);
        }

        /// <summary>
        /// Asynchronously reads all available samples without removing them.
        /// </summary>
        public Task<IReadOnlyList<Sample<T>>> ReadAsync(CancellationToken cancellationToken = default)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            return Task.Run(() => _reader.Read(), cancellationToken);
        }

        /// <summary>
        /// Waits for data to become available on the reader.
        /// </summary>
        /// <param name="timeout">Maximum time to wait. Pass <c>null</c> for infinite wait.</param>
        /// <param name="cancellationToken">Token to cancel the wait.</param>
        /// <returns><c>true</c> if data is available; <c>false</c> if the timeout expired.</returns>
        public async Task<bool> WaitForDataAsync(TimeSpan? timeout = null, CancellationToken cancellationToken = default)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

            if (_waitSet == null)
            {
                _waitSet = new AsyncWaitSet();
                _waitSet.AttachDataReader(_reader.Handle);
            }

            return await _waitSet.WaitAsync(timeout, cancellationToken).ConfigureAwait(false);
        }

        /// <summary>
        /// Returns an async enumerable that yields samples as they become available.
        /// The enumeration continues indefinitely until cancelled.
        /// </summary>
        public async IAsyncEnumerable<Sample<T>> ReadAllAsync(
            [EnumeratorCancellation] CancellationToken cancellationToken = default)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

            while (!cancellationToken.IsCancellationRequested)
            {
                var samples = await TakeAsync(cancellationToken).ConfigureAwait(false);

                foreach (var sample in samples)
                {
                    yield return sample;
                }

                if (samples.Count == 0)
                {
                    // No data available — wait before polling again.
                    await WaitForDataAsync(TimeSpan.FromMilliseconds(100), cancellationToken).ConfigureAwait(false);
                }
            }
        }

        public void Dispose()
        {
            if (_disposed) return;
            _disposed = true;
            _waitSet?.Dispose();
            _waitSet = null;
        }

        public ValueTask DisposeAsync()
        {
            Dispose();
            return new ValueTask();
        }
    }
}
