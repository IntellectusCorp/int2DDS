using System;
using System.Collections;
using System.Collections.Generic;
using Int2Dds.Exceptions;
using Int2Dds.Interop;
using Int2Dds.Types;

namespace Int2Dds.Core
{
    internal sealed class LoanState
    {
        internal bool Returned;

        internal void ThrowIfReturned()
        {
            if (Returned) throw new ObjectDisposedException("LoanedSamples", "The loan has been returned.");
        }
    }

    /// <summary>One loaned element: its SampleInfo and the lent serialized bytes.</summary>
    public sealed class LoanedSample
    {
        private readonly LoanState _state;
        private readonly SampleInfo _info;
        private readonly IntPtr _data;
        private readonly int _size;

        internal LoanedSample(LoanState state, SampleInfo info, IntPtr data, int size)
        {
            _state = state;
            _info = info;
            _data = data;
            _size = size;
        }

        public SampleInfo Info
        {
            get { _state.ThrowIfReturned(); return _info; }
        }

        /// <summary>
        /// The sample's CDR bytes in native memory; empty when <c>Info.ValidData</c> is false.
        /// Valid only until the loan is returned.
        /// </summary>
        public unsafe ReadOnlySpan<byte> SerializedData
        {
            get
            {
                _state.ThrowIfReturned();
                return _data == IntPtr.Zero ? ReadOnlySpan<byte>.Empty : new ReadOnlySpan<byte>((void*)_data, _size);
            }
        }
    }

    /// <summary>
    /// Samples loaned from a DataReader's cache without copying (DDS v1.4 2.2.2.5.3.20).
    /// Dispose (or <see cref="DataReader{T}.ReturnLoan"/>) returns the loan.
    /// </summary>
    public sealed class LoanedSamples<T> : IDisposable, IReadOnlyList<LoanedSample> where T : class, IDdsType, new()
    {
        private readonly DataReader<T> _reader;
        private readonly LoanState _state = new LoanState();
        private readonly LoanedSample[] _samples;
        private IntPtr _handle;

        internal unsafe LoanedSamples(DataReader<T> reader, IntPtr handle)
        {
            _reader = reader;
            _handle = handle;
            var length = handle == IntPtr.Zero ? 0 : (int)(ulong)NativeMethods.int2dds_loaned_samples_length(handle);
            _samples = new LoanedSample[length];
            try
            {
                for (var i = 0; i < length; i++)
                {
                    NativeSampleInfo nativeInfo;
                    ReturnCodeHelper.CheckReturn(
                        NativeMethods.int2dds_loaned_samples_get_info(handle, (UIntPtr)i, &nativeInfo));
                    byte* data = null;
                    var size = UIntPtr.Zero;
                    if (nativeInfo.ValidData)
                        ReturnCodeHelper.CheckReturn(
                            NativeMethods.int2dds_loaned_samples_get_data(handle, (UIntPtr)i, out data, out size));
                    _samples[i] = new LoanedSample(
                        _state, DataReader<T>.ConvertSampleInfo(ref nativeInfo), (IntPtr)data, (int)(ulong)size);
                }
            }
            catch
            {
                NativeMethods.int2dds_datareader_return_loan(reader.Handle, handle);
                throw;
            }
        }

        internal IntPtr Handle => _handle;

        internal bool IsReturned => _state.Returned;

        internal void MarkReturned()
        {
            _state.Returned = true;
            _handle = IntPtr.Zero;
        }

        public int Count
        {
            get { _state.ThrowIfReturned(); return _samples.Length; }
        }

        public LoanedSample this[int index]
        {
            get { _state.ThrowIfReturned(); return _samples[index]; }
        }

        public IEnumerator<LoanedSample> GetEnumerator()
        {
            _state.ThrowIfReturned();
            return ((IEnumerable<LoanedSample>)_samples).GetEnumerator();
        }

        IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();

        /// <summary>Returns the loan to its reader. Idempotent.</summary>
        public void Dispose() => _reader.ReturnLoan(this);
    }
}
