using System;
using System.Threading;

namespace Int2Dds.Core
{
    internal static class WaitTimeout
    {
        // Timeout.InfiniteTimeSpan waits forever; spans beyond the native range saturate
        // to infinite in the FFI, and a negative span passes through and has already expired.
        internal static long ToMilliseconds(TimeSpan value)
        {
            if (value == Timeout.InfiniteTimeSpan)
                return long.MaxValue;
            return value.Ticks / TimeSpan.TicksPerMillisecond;
        }
    }
}
