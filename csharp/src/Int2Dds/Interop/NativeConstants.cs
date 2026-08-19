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
