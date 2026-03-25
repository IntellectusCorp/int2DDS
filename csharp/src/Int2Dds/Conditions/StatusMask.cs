using System;

namespace Int2Dds.Conditions
{
    /// <summary>
    /// Bit flags representing DDS status kinds for conditions and listeners.
    /// </summary>
    [Flags]
    public enum StatusMask : uint
    {
        None = 0,
        InconsistentTopic = 1 << 0,
        OfferedDeadlineMissed = 1 << 1,
        RequestedDeadlineMissed = 1 << 2,
        OfferedIncompatibleQos = 1 << 5,
        RequestedIncompatibleQos = 1 << 6,
        SampleLost = 1 << 7,
        SampleRejected = 1 << 8,
        DataOnReaders = 1 << 9,
        DataAvailable = 1 << 10,
        LivelinessLost = 1 << 11,
        LivelinessChanged = 1 << 12,
        PublicationMatched = 1 << 13,
        SubscriptionMatched = 1 << 14,
        All = 0xFFFFFFFF,
    }
}
