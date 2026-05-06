// Copyright IntellectusCorp. All rights reserved.
// Licensed under the int2DDS license.

namespace Int2Dds.Internal
{
    // Cached empty arrays. Replacement for Array.Empty<T>() which is .NET 4.6+ only;
    // we use this universally so the codebase stays the same on every TFM (including net45).
    internal static class EmptyArrayHolder<T>
    {
        public static readonly T[] Value = new T[0];
    }
}
