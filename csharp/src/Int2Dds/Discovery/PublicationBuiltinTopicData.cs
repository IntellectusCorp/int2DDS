using System;
using System.Text;
using Int2Dds.Exceptions;
using Int2Dds.Interop;

namespace Int2Dds.Discovery
{
    /// <summary>
    /// Wrapper around a discovered publication's builtin topic data.
    /// </summary>
    public sealed class PublicationBuiltinTopicData : IDisposable
    {
        private IntPtr _handle;
        private bool _disposed;

        internal PublicationBuiltinTopicData(IntPtr handle)
        {
            _handle = handle;
        }

        public unsafe byte[] Key
        {
            get
            {
                if (_disposed) throw new ObjectDisposedException(GetType().Name);
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
                if (_disposed) throw new ObjectDisposedException(GetType().Name);
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
                if (_disposed) throw new ObjectDisposedException(GetType().Name);
                UIntPtr size;
                var buf = new byte[256];
                fixed (byte* p = buf)
                {
                    int ret = NativeMethods.int2dds_publication_builtin_topic_data_get_topic_name(_handle, p, (UIntPtr)256, out size);
                    if (ret == ReturnCode.NoData || (uint)size == 0) return string.Empty;
                    ReturnCodeHelper.CheckReturn(ret);
                }
                int len = (int)(uint)size;
                if (len > 0 && buf[len - 1] == 0) len--;
                return Encoding.UTF8.GetString(buf, 0, len);
            }
        }

        public unsafe string TypeName
        {
            get
            {
                if (_disposed) throw new ObjectDisposedException(GetType().Name);
                UIntPtr size;
                var buf = new byte[256];
                fixed (byte* p = buf)
                {
                    int ret = NativeMethods.int2dds_publication_builtin_topic_data_get_type_name(_handle, p, (UIntPtr)256, out size);
                    if (ret == ReturnCode.NoData || (uint)size == 0) return string.Empty;
                    ReturnCodeHelper.CheckReturn(ret);
                }
                int len = (int)(uint)size;
                if (len > 0 && buf[len - 1] == 0) len--;
                return Encoding.UTF8.GetString(buf, 0, len);
            }
        }

        /// <summary>The 16-byte endpoint GUID of the discovered writer.</summary>
        public unsafe byte[] EndpointGuid
        {
            get
            {
                if (_disposed) throw new ObjectDisposedException(GetType().Name);
                var guid = new byte[16];
                fixed (byte* p = guid)
                {
                    ReturnCodeHelper.CheckReturn(
                        NativeMethods.int2dds_publication_builtin_topic_data_get_endpoint_guid(_handle, p));
                }
                return guid;
            }
        }

        /// <summary>Reliability kind: 0 = BEST_EFFORT, 1 = RELIABLE.</summary>
        public int ReliabilityKind
        {
            get
            {
                if (_disposed) throw new ObjectDisposedException(GetType().Name);
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_publication_builtin_topic_data_get_reliability_kind(_handle, out var kind));
                return kind;
            }
        }

        /// <summary>Durability kind: 0 = VOLATILE, 1 = TRANSIENT_LOCAL, 2 = TRANSIENT, 3 = PERSISTENT.</summary>
        public int DurabilityKind
        {
            get
            {
                if (_disposed) throw new ObjectDisposedException(GetType().Name);
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_publication_builtin_topic_data_get_durability_kind(_handle, out var kind));
                return kind;
            }
        }

        /// <summary>Liveliness kind: 0 = AUTOMATIC, 1 = MANUAL_BY_PARTICIPANT, 2 = MANUAL_BY_TOPIC.</summary>
        public int LivelinessKind
        {
            get
            {
                if (_disposed) throw new ObjectDisposedException(GetType().Name);
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_publication_builtin_topic_data_get_liveliness_kind(_handle, out var kind));
                return kind;
            }
        }

        /// <summary>Liveliness lease duration; <c>null</c> means infinite.</summary>
        public TimeSpan? LivelinessLeaseDuration
        {
            get
            {
                if (_disposed) throw new ObjectDisposedException(GetType().Name);
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_publication_builtin_topic_data_get_liveliness_lease_duration(_handle, out var sec, out var nanosec));
                return DurationToTimeSpan(sec, nanosec);
            }
        }

        /// <summary>Deadline period; <c>null</c> means infinite.</summary>
        public TimeSpan? Deadline
        {
            get
            {
                if (_disposed) throw new ObjectDisposedException(GetType().Name);
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_publication_builtin_topic_data_get_deadline(_handle, out var sec, out var nanosec));
                return DurationToTimeSpan(sec, nanosec);
            }
        }

        /// <summary>Lifespan duration; <c>null</c> means infinite.</summary>
        public TimeSpan? Lifespan
        {
            get
            {
                if (_disposed) throw new ObjectDisposedException(GetType().Name);
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_publication_builtin_topic_data_get_lifespan(_handle, out var sec, out var nanosec));
                return DurationToTimeSpan(sec, nanosec);
            }
        }

        /// <summary>The writer's user_data bytes (possibly empty).</summary>
        public unsafe byte[] UserData
        {
            get
            {
                if (_disposed) throw new ObjectDisposedException(GetType().Name);
                UIntPtr size;
                NativeMethods.int2dds_publication_builtin_topic_data_get_user_data(_handle, null, UIntPtr.Zero, out size);
                int len = (int)(uint)size;
                if (len == 0) return new byte[0];
                var buf = new byte[len];
                fixed (byte* p = buf)
                {
                    ReturnCodeHelper.CheckReturn(
                        NativeMethods.int2dds_publication_builtin_topic_data_get_user_data(_handle, p, (UIntPtr)len, out size));
                }
                return buf;
            }
        }

        internal static TimeSpan? DurationToTimeSpan(int sec, uint nanosec)
        {
            if (sec == int.MaxValue && nanosec == 0x7fffffff) return null; // infinite
            return TimeSpan.FromSeconds(sec) + TimeSpan.FromTicks(nanosec / 100);
        }

        public void Dispose()
        {
            if (_disposed) return;
            _disposed = true;
            if (_handle != IntPtr.Zero)
            {
                NativeMethods.int2dds_publication_builtin_topic_data_destroy(_handle);
                _handle = IntPtr.Zero;
            }
        }
    }
}
