using System;
using Int2Dds.Core;
using Int2Dds.Exceptions;
using Int2Dds.Interop;

namespace Int2Dds.Xtypes
{
    /// <summary>
    /// A DataWriter bound to a <see cref="DynamicTypeSupport"/>. It publishes
    /// <see cref="DynamicData"/> samples whose type is defined purely at runtime.
    /// </summary>
    public sealed class DynamicTypeWriter : IDisposable
    {
        private IntPtr _handle;

        internal DynamicTypeWriter(IntPtr handle)
        {
            _handle = handle;
        }

        /// <summary>Publish a populated <see cref="DynamicData"/> sample.</summary>
        public void Write(DynamicData data)
        {
            if (data == null) throw new ArgumentNullException(nameof(data));
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_writer_write(_handle, data.Handle));
        }

        /// <summary>Current number of matched readers.</summary>
        public int PublicationMatchedCount()
        {
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_writer_publication_matched_count(_handle, out int count));
            return count;
        }

        public void Dispose()
        {
            if (_handle != IntPtr.Zero)
            {
                NativeMethods.int2dds_dynamic_writer_destroy(_handle);
                _handle = IntPtr.Zero;
            }
        }
    }

    /// <summary>
    /// A DataReader bound to a <see cref="DynamicTypeSupport"/>. It returns received samples
    /// as <see cref="DynamicData"/> instances.
    /// </summary>
    public sealed class DynamicTypeReader : IDisposable
    {
        private IntPtr _handle;

        internal DynamicTypeReader(IntPtr handle)
        {
            _handle = handle;
        }

        /// <summary>Take the next available sample, or <c>null</c> when none is ready.</summary>
        public DynamicData? Take()
        {
            int ret = NativeMethods.int2dds_dynamic_reader_take(_handle, out IntPtr data, IntPtr.Zero);
            if (ret == ReturnCode.NoData)
                return null;
            ReturnCodeHelper.CheckReturn(ret);
            return new DynamicData(data);
        }

        /// <summary>Current number of matched writers.</summary>
        public int SubscriptionMatchedCount()
        {
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_reader_subscription_matched_count(_handle, out int count));
            return count;
        }

        public void Dispose()
        {
            if (_handle != IntPtr.Zero)
            {
                NativeMethods.int2dds_dynamic_reader_destroy(_handle);
                _handle = IntPtr.Zero;
            }
        }
    }

    /// <summary>
    /// Participant/Publisher/Subscriber extension methods for the XML/type-support-backed
    /// dynamic path: a topic created from a <see cref="DynamicTypeSupport"/> plus writers and
    /// readers that exchange <see cref="DynamicData"/> directly.
    /// </summary>
    public static class DynamicTypeEndpoints
    {
        /// <summary>Create a topic backed by a <see cref="DynamicTypeSupport"/>.</summary>
        public static unsafe DynamicTopic CreateTopicDynamic(
            this DomainParticipant participant, string topicName, DynamicTypeSupport support)
        {
            if (support == null) throw new ArgumentNullException(nameof(support));
            var tb = NativeString.ToCStr(topicName);
            fixed (byte* p = tb)
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_create_topic_dynamic(
                    participant.Handle, p, support.Handle, IntPtr.Zero, out IntPtr topic));
                return new DynamicTopic(topic, topicName);
            }
        }

        /// <summary>Create a dynamic DataWriter for the given dynamic topic.</summary>
        public static DynamicTypeWriter CreateDataWriterDynamic(
            this Publisher publisher, DynamicTopic topic, DynamicTypeSupport support)
        {
            if (topic == null) throw new ArgumentNullException(nameof(topic));
            if (support == null) throw new ArgumentNullException(nameof(support));
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_create_datawriter_dynamic(
                publisher.Handle, topic.Handle, support.Handle, IntPtr.Zero, out IntPtr writer));
            return new DynamicTypeWriter(writer);
        }

        /// <summary>Create a dynamic DataReader for the given dynamic topic.</summary>
        public static DynamicTypeReader CreateDataReaderDynamic(
            this Subscriber subscriber, DynamicTopic topic, DynamicTypeSupport support)
        {
            if (topic == null) throw new ArgumentNullException(nameof(topic));
            if (support == null) throw new ArgumentNullException(nameof(support));
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_create_datareader_dynamic(
                subscriber.Handle, topic.Handle, support.Handle, IntPtr.Zero, out IntPtr reader));
            return new DynamicTypeReader(reader);
        }
    }
}
