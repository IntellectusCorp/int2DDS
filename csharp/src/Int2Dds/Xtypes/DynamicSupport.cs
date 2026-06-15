using System;
using System.Text;
using Int2Dds.Core;
using Int2Dds.Exceptions;
using Int2Dds.Interop;

namespace Int2Dds.Xtypes
{
    /// <summary>
    /// Participant-level helpers for runtime dynamic types: discovering a remote
    /// TypeObject and decoding received samples into <see cref="DynamicData"/>.
    /// </summary>
    public static class DynamicSupport
    {
        /// <summary>
        /// Block until a publication for <paramref name="topicName"/> is discovered
        /// with a TypeObject. Returns the TypeObject and the discovered type name.
        /// </summary>
        public static unsafe DynamicTypeObject WaitForTypeObject(
            this DomainParticipant participant, string topicName, int timeoutMs, out string typeName)
        {
            var topicBytes = Encoding.UTF8.GetBytes(topicName + '\0');
            byte[] nameBuf = new byte[256];
            UIntPtr outLen;
            IntPtr typeObj;
            fixed (byte* pTopic = topicBytes)
            fixed (byte* pName = nameBuf)
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_wait_for_type_object(
                    participant.Handle, pTopic, timeoutMs, out typeObj, pName, (UIntPtr)nameBuf.Length, out outLen));
            }
            typeName = Encoding.UTF8.GetString(nameBuf, 0, (int)outLen);
            return new DynamicTypeObject(typeObj);
        }

        /// <summary>
        /// Decode raw CDR <paramref name="data"/> into a <see cref="DynamicData"/>
        /// using <paramref name="typeObj"/> and the participant's type registry
        /// (for nested member resolution).
        /// </summary>
        public static unsafe DynamicData DecodeSample(
            this DomainParticipant participant, DynamicTypeObject typeObj, byte[] data)
        {
            fixed (byte* pData = data)
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_dynamic_data_from_sample(
                    participant.Handle, pData, (UIntPtr)data.Length, typeObj.Handle, out IntPtr dd));
                return new DynamicData(dd);
            }
        }
    }
}
