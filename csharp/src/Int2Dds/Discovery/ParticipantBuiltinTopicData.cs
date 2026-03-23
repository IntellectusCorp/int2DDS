using System.Runtime.InteropServices;
using Int2Dds.Exceptions;
using Int2Dds.Interop;

namespace Int2Dds.Discovery;

/// <summary>
/// Wrapper around a discovered participant's builtin topic data.
/// </summary>
public sealed class ParticipantBuiltinTopicData : IDisposable
{
    private nint _handle;
    private bool _disposed;

    internal ParticipantBuiltinTopicData(nint handle)
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
            ObjectDisposedException.ThrowIf(_disposed, this);
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
            ObjectDisposedException.ThrowIf(_disposed, this);

            // First call to get the size.
            int ret = NativeMethods.int2dds_participant_builtin_topic_data_get_user_data(
                _handle, null, 0, out nuint size);

            if (ret == ReturnCode.NoData || size == 0)
                return [];

            // Some implementations return OK with size, some may require a buffer.
            var buf = new byte[size];
            fixed (byte* p = buf)
            {
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_participant_builtin_topic_data_get_user_data(
                        _handle, p, size, out _));
            }
            return buf;
        }
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;

        if (_handle != 0)
        {
            NativeMethods.int2dds_participant_builtin_topic_data_destroy(_handle);
            _handle = 0;
        }
    }
}
