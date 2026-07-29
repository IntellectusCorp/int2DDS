namespace Int2Dds.Conditions
{
    /// <summary>
    /// Sample state mask bits (matching the DDS SampleStateKind bitflags).
    /// </summary>
    public static class SampleState
    {
        public const uint Read = 0x0001;
        public const uint NotRead = 0x0002;
        public const uint Any = 0xFFFF;
    }

    /// <summary>
    /// View state mask bits (matching the DDS ViewStateKind bitflags).
    /// </summary>
    public static class ViewState
    {
        public const uint New = 0x0001;
        public const uint NotNew = 0x0002;
        public const uint Any = 0xFFFF;
    }

    /// <summary>
    /// Instance state mask bits (matching the DDS InstanceStateKind bitflags).
    /// </summary>
    public static class InstanceState
    {
        public const uint Alive = 0x0001;
        public const uint NotAliveDisposed = 0x0002;
        public const uint NotAliveNoWriters = 0x0004;
        public const uint Any = 0xFFFF;
    }
}
