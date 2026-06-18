using System;
using System.Text;
using Int2Dds.Exceptions;

namespace Int2Dds.Interop
{
    /// <summary>
    /// Helpers for marshaling strings across the FFI boundary.
    /// </summary>
    internal static class NativeString
    {
        /// <summary>UTF-8 bytes with a trailing NUL, ready to <c>fixed</c> and pass as <c>const char*</c>.</summary>
        internal static byte[] ToCStr(string s) => Encoding.UTF8.GetBytes((s ?? string.Empty) + '\0');

        internal unsafe delegate int BufferReader(byte* buf, UIntPtr cap, out UIntPtr outLen);

        /// <summary>
        /// Invoke a <c>(char *buf, uintptr_t cap, uintptr_t *out_len)</c> getter, sizing the
        /// buffer from the reported required length (retrying once when it didn't fit).
        /// </summary>
        internal static unsafe string Read(BufferReader reader)
        {
            byte[] buf = new byte[256];
            UIntPtr outLen;
            fixed (byte* p = buf)
            {
                int ret = reader(p, (UIntPtr)buf.Length, out outLen);
                if (ret == ReturnCode.Ok)
                    return Encoding.UTF8.GetString(buf, 0, (int)outLen);
            }
            int need = (int)outLen;
            buf = new byte[need + 1];
            fixed (byte* p = buf)
            {
                ReturnCodeHelper.CheckReturn(reader(p, (UIntPtr)buf.Length, out outLen));
                return Encoding.UTF8.GetString(buf, 0, (int)outLen);
            }
        }
    }
}
