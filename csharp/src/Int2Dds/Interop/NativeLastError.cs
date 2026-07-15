using System;
using System.Runtime.InteropServices;
using System.Text;

namespace Int2Dds.Interop
{
    internal static partial class NativeMethods
    {
        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_last_error_message(byte* buf, int buf_len);
    }

    /// <summary>
    /// Calling thread's last FFI error message. Call on the same thread,
    /// immediately after a failing FFI call.
    /// </summary>
    internal static class NativeLastError
    {
        internal static unsafe string GetMessage()
        {
            int len = NativeMethods.int2dds_last_error_message(null, 0); // query length
            if (len <= 0)
                return string.Empty;

            byte[] buf = new byte[len + 1];
            fixed (byte* p = buf)
            {
                int written = NativeMethods.int2dds_last_error_message(p, buf.Length);
                if (written <= 0)
                    return string.Empty;
                int n = Math.Min(written, buf.Length - 1);
                return Encoding.UTF8.GetString(buf, 0, n);
            }
        }
    }
}
