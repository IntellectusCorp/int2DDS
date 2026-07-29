using System;
using System.Runtime.InteropServices;
using Int2Dds.Exceptions;
using Int2Dds.Interop;

namespace Int2Dds.Discovery
{
    /// <summary>
    /// Wrapper around a discovered participant's builtin topic data.
    /// </summary>
    public sealed class ParticipantBuiltinTopicData : IDisposable
    {
        private IntPtr _handle;
        private bool _disposed;

        internal ParticipantBuiltinTopicData(IntPtr handle)
        {
            _handle = handle;
        }

        /// <summary>
        /// Gets the 12-byte key identifying this participant.
        /// </summary>
        public unsafe byte[] Key
        {
            get
            {
                if (_disposed) throw new ObjectDisposedException(GetType().Name);
                var key = new byte[12];
                fixed (byte* p = key)
                {
                    ReturnCodeHelper.CheckReturn(
                        NativeMethods.int2dds_participant_builtin_topic_data_get_key(_handle, p));
                }
                return key;
            }
        }

        /// <summary>
        /// Gets the user data attached to this participant.
        /// </summary>
        public unsafe byte[] UserData
        {
            get
            {
                if (_disposed) throw new ObjectDisposedException(GetType().Name);

                // First call to get the size.
                int ret = NativeMethods.int2dds_participant_builtin_topic_data_get_user_data(
                    _handle, null, UIntPtr.Zero, out UIntPtr size);

                if (ret == ReturnCode.NoData || (uint)size == 0)
                    return Int2Dds.Internal.EmptyArrayHolder<byte>.Value;

                // Some implementations return OK with size, some may require a buffer.
                var buf = new byte[(uint)size];
                fixed (byte* p = buf)
                {
                    UIntPtr dummy;
                    ReturnCodeHelper.CheckReturn(
                        NativeMethods.int2dds_participant_builtin_topic_data_get_user_data(
                            _handle, p, size, out dummy));
                }
                return buf;
            }
        }

        public void Dispose()
        {
            if (_disposed) return;
            _disposed = true;

            if (_handle != IntPtr.Zero)
            {
                NativeMethods.int2dds_participant_builtin_topic_data_destroy(_handle);
                _handle = IntPtr.Zero;
            }
        }
    }
}
