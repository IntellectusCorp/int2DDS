using System;

namespace Int2Dds.Interop
{
    /// <summary>
    /// FFI return codes from int2dds-ffi.h
    /// </summary>
    internal static class ReturnCode
    {
        public const int Ok = 0;
        public const int Error = 1;
        public const int Timeout = 2;
        public const int Unsupported = 3;
        public const int InvalidArgument = 11;
        public const int AlreadyDeleted = 20;
        public const int NotEnabled = 21;
        public const int ImmutablePolicy = 22;
        public const int InconsistentPolicy = 23;
        public const int PreconditionNotMet = 24;
        public const int OutOfResources = 25;
        public const int IllegalOperation = 26;
        public const int NoData = 27;
        public const int NullPointer = 100;
        public const int BufferTooSmall = 101;
    }

    /// <summary>
    /// QoS Reliability kind constants
    /// </summary>
    internal static class QosReliability
    {
        public const int BestEffort = 0;
        public const int Reliable = 1;
    }

    /// <summary>
    /// QoS Durability kind constants
    /// </summary>
    internal static class QosDurability
    {
        public const int Volatile = 0;
        public const int TransientLocal = 1;
        public const int Transient = 2;
        public const int Persistent = 3;
    }

    /// <summary>
    /// QoS History kind constants
    /// </summary>
    internal static class QosHistory
    {
        public const int KeepLast = 0;
        public const int KeepAll = 1;
    }

    /// <summary>
    /// QoS Data Representation kind constants
    /// </summary>
    internal static class QosDataRepresentation
    {
        public const int Xcdr1 = 0;
        public const int Xcdr2 = 2;
    }

    /// <summary>
    /// QoS Liveliness kind constants
    /// </summary>
    internal static class QosLiveliness
    {
        public const int Automatic = 0;
        public const int ManualByParticipant = 1;
        public const int ManualByTopic = 2;
    }

    /// <summary>
    /// QoS Ownership kind constants
    /// </summary>
    internal static class QosOwnership
    {
        public const int Shared = 0;
        public const int Exclusive = 1;
    }

    /// <summary>
    /// QoS Destination Order kind constants
    /// </summary>
    internal static class QosDestinationOrder
    {
        public const int ByReception = 0;
        public const int BySource = 1;
    }

    /// <summary>
    /// Status mask bit constants
    /// </summary>
    internal static class StatusMaskBits
    {
        public const uint InconsistentTopic = 1 << 0;
        public const uint OfferedDeadlineMissed = 1 << 1;
        public const uint RequestedDeadlineMissed = 1 << 2;
        public const uint OfferedIncompatibleQos = 1 << 5;
        public const uint RequestedIncompatibleQos = 1 << 6;
        public const uint SampleLost = 1 << 7;
        public const uint SampleRejected = 1 << 8;
        public const uint DataOnReaders = 1 << 9;
        public const uint DataAvailable = 1 << 10;
        public const uint LivelinessLost = 1 << 11;
        public const uint LivelinessChanged = 1 << 12;
        public const uint PublicationMatched = 1 << 13;
        public const uint SubscriptionMatched = 1 << 14;
        public const uint StatusMaskAll = 0xFFFFFFFF;
        public const uint StatusMaskNone = 0;
    }

    /// <summary>
    /// Sample state constants
    /// </summary>
    internal static class SampleState
    {
        public const uint Read = 1;
        public const uint NotRead = 2;
        public const uint Any = 65535;
    }

    /// <summary>
    /// View state constants
    /// </summary>
    internal static class ViewState
    {
        public const uint New = 1;
        public const uint NotNew = 2;
        public const uint Any = 65535;
    }

    /// <summary>
    /// Instance state constants
    /// </summary>
    internal static class InstanceState
    {
        public const uint Alive = 1;
        public const uint NotAliveDisposed = 2;
        public const uint NotAliveNoWriters = 4;
        public const uint Any = 65535;
    }

    /// <summary>
    /// Field type constants for TypeInfo builder
    /// </summary>
    internal static class FieldTypeConstants
    {
        public const int Bool = 0;
        public const int Byte = 1;
        public const int Char8 = 2;
        public const int Int8 = 3;
        public const int Int16 = 4;
        public const int Int32 = 5;
        public const int Int64 = 6;
        public const int UInt8 = 7;
        public const int UInt16 = 8;
        public const int UInt32 = 9;
        public const int UInt64 = 10;
        public const int Float32 = 11;
        public const int Float64 = 12;
        public const int String = 13;
        public const int Enum = 14;
    }

    /// <summary>
    /// QoS Policy ID enum (for incompatible QoS status).
    /// A field of two native structs, so the width is stated rather than defaulted.
    /// </summary>
    internal enum QosPolicyId : int
    {
        Invalid = 0,
        UserData = 1,
        Durability = 2,
        Presentation = 3,
        Deadline = 4,
        LatencyBudget = 5,
        Ownership = 6,
        OwnershipStrength = 7,
        Liveliness = 8,
        TimeBasedFilter = 9,
        Partition = 10,
        Reliability = 11,
        DestinationOrder = 12,
        History = 13,
        ResourceLimits = 14,
        EntityFactory = 15,
        WriterDataLifecycle = 16,
        ReaderDataLifecycle = 17,
        TopicData = 18,
        GroupData = 19,
        TransportPriority = 20,
        Lifespan = 21,
        DurabilityService = 22,
        DataRepresentation = 23,
        TypeConsistencyEnforcement = 24,
        Property = 25,
    }

    /// <summary>
    /// Sample rejected status kind. A field of a native struct, as above.
    /// </summary>
    internal enum SampleRejectedStatusKind : int
    {
        NotRejected = 0,
        RejectedByInstancesLimit = 1,
        RejectedBySamplesLimit = 2,
        RejectedBySamplesPerInstanceLimit = 3,
    }

    /// <summary>
    /// CDR encapsulation IDs
    /// </summary>
    internal static class CdrEncapsulation
    {
        public const ushort CdrBe = 0x0000;
        public const ushort CdrLe = 0x0001;
        public const ushort PlCdrBe = 0x0002;
        public const ushort PlCdrLe = 0x0003;
        public const ushort Cdr2Be = 0x0006;
        public const ushort Cdr2Le = 0x0007;
        public const ushort DCdr2Be = 0x0008;
        public const ushort DCdr2Le = 0x0009;
        public const ushort PlCdr2Be = 0x000A;
        public const ushort PlCdr2Le = 0x000B;
    }

    /// <summary>
    /// CDR extensibility kinds
    /// </summary>
    internal static class CdrExtensibility
    {
        public const int Final = 0;
        public const int Appendable = 1;
        public const int Mutable = 2;
    }

    /// <summary>
    /// CDR EMHEADER sentinel member ID
    /// </summary>
    internal static class CdrSentinel
    {
        public const uint MemberIdSentinel = 0x3F02;
    }
}
