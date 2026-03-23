using System.Text;
using Int2Dds.Exceptions;
using Int2Dds.Interop;

namespace Int2Dds.Discovery;

/// <summary>
/// Wrapper around a discovered publication's builtin topic data.
/// </summary>
public sealed class PublicationBuiltinTopicData : IDisposable
{
    private nint _handle;
    private bool _disposed;

    internal PublicationBuiltinTopicData(nint handle)
    {
        _handle = handle;
    }

    public unsafe byte[] Key
    {
        get
        {
            ObjectDisposedException.ThrowIf(_disposed, this);
            var key = new byte[12];
            fixed (byte* p = key)
            {
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_publication_builtin_topic_data_get_key(_handle, p));
            }
            return key;
        }
    }

    public unsafe byte[] ParticipantKey
    {
        get
        {
            ObjectDisposedException.ThrowIf(_disposed, this);
            var key = new byte[12];
            fixed (byte* p = key)
            {
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_publication_builtin_topic_data_get_participant_key(_handle, p));
            }
            return key;
        }
    }

    public unsafe string TopicName
    {
        get
        {
            ObjectDisposedException.ThrowIf(_disposed, this);
            nuint size;
            var buf = new byte[256];
            fixed (byte* p = buf)
            {
                int ret = NativeMethods.int2dds_publication_builtin_topic_data_get_topic_name(_handle, p, 256, out size);
                if (ret == ReturnCode.NoData || size == 0) return string.Empty;
                ReturnCodeHelper.CheckReturn(ret);
            }
            int len = (int)size;
            if (len > 0 && buf[len - 1] == 0) len--;
            return Encoding.UTF8.GetString(buf, 0, len);
        }
    }

    public unsafe string TypeName
    {
        get
        {
            ObjectDisposedException.ThrowIf(_disposed, this);
            nuint size;
            var buf = new byte[256];
            fixed (byte* p = buf)
            {
                int ret = NativeMethods.int2dds_publication_builtin_topic_data_get_type_name(_handle, p, 256, out size);
                if (ret == ReturnCode.NoData || size == 0) return string.Empty;
                ReturnCodeHelper.CheckReturn(ret);
            }
            int len = (int)size;
            if (len > 0 && buf[len - 1] == 0) len--;
            return Encoding.UTF8.GetString(buf, 0, len);
        }
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        if (_handle != 0)
        {
            NativeMethods.int2dds_publication_builtin_topic_data_destroy(_handle);
            _handle = 0;
        }
    }
}
