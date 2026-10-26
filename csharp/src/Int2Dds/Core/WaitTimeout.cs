using System;
using System.Threading;

namespace Int2Dds.Core
{
    internal static class WaitTimeout
    {
        // Timeout.InfiniteTimeSpan and spans beyond the native range wait forever;
        // a negative span passes through and has already expired.
        internal static long ToMilliseconds(TimeSpan value)
        {
            if (value == Timeout.InfiniteTimeSpan || value.Ticks >= long.MaxValue / TimeSpan.TicksPerMillisecond)
                return long.MaxValue;
            return value.Ticks / TimeSpan.TicksPerMillisecond;
        }
    }
}
