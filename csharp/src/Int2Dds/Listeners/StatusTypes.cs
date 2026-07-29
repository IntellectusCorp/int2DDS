namespace Int2Dds.Listeners
{
    /// <summary>Status of publication matching for a DataWriter.</summary>
    public class PublicationMatchedStatus
    {
        public int TotalCount { get; }
        public int TotalCountChange { get; }
        public int CurrentCount { get; }
        public int CurrentCountChange { get; }

        public PublicationMatchedStatus(int totalCount, int totalCountChange, int currentCount, int currentCountChange)
        {
            TotalCount = totalCount;
            TotalCountChange = totalCountChange;
            CurrentCount = currentCount;
            CurrentCountChange = currentCountChange;
        }
    }

    /// <summary>Status of subscription matching for a DataReader.</summary>
    public class SubscriptionMatchedStatus
    {
        public int TotalCount { get; }
        public int TotalCountChange { get; }
        public int CurrentCount { get; }
        public int CurrentCountChange { get; }

        public SubscriptionMatchedStatus(int totalCount, int totalCountChange, int currentCount, int currentCountChange)
        {
            TotalCount = totalCount;
            TotalCountChange = totalCountChange;
            CurrentCount = currentCount;
            CurrentCountChange = currentCountChange;
        }
    }

    /// <summary>Status when a DataWriter misses its offered deadline.</summary>
    public class OfferedDeadlineMissedStatus
    {
        public int TotalCount { get; }
        public int TotalCountChange { get; }

        public OfferedDeadlineMissedStatus(int totalCount, int totalCountChange)
        {
            TotalCount = totalCount;
            TotalCountChange = totalCountChange;
        }
    }

    /// <summary>Status when a DataReader misses a requested deadline.</summary>
    public class RequestedDeadlineMissedStatus
    {
        public int TotalCount { get; }
        public int TotalCountChange { get; }

        public RequestedDeadlineMissedStatus(int totalCount, int totalCountChange)
        {
            TotalCount = totalCount;
            TotalCountChange = totalCountChange;
        }
    }

    /// <summary>Status when a DataWriter loses liveliness.</summary>
    public class LivelinessLostStatus
    {
        public int TotalCount { get; }
        public int TotalCountChange { get; }

        public LivelinessLostStatus(int totalCount, int totalCountChange)
        {
            TotalCount = totalCount;
            TotalCountChange = totalCountChange;
        }
    }

    /// <summary>Status when liveliness changes for a DataReader.</summary>
    public class LivelinessChangedStatus
    {
        public int AliveCount { get; }
        public int NotAliveCount { get; }
        public int AliveCountChange { get; }
        public int NotAliveCountChange { get; }

        public LivelinessChangedStatus(int aliveCount, int notAliveCount, int aliveCountChange, int notAliveCountChange)
        {
            AliveCount = aliveCount;
            NotAliveCount = notAliveCount;
            AliveCountChange = aliveCountChange;
            NotAliveCountChange = notAliveCountChange;
        }
    }

    /// <summary>Status when samples are lost.</summary>
    public class SampleLostStatus
    {
        public int TotalCount { get; }
        public int TotalCountChange { get; }

        public SampleLostStatus(int totalCount, int totalCountChange)
        {
            TotalCount = totalCount;
            TotalCountChange = totalCountChange;
        }
    }

    /// <summary>Status when a topic with the same name but an incompatible type is discovered.</summary>
    public class InconsistentTopicStatus
    {
        public int TotalCount { get; }
        public int TotalCountChange { get; }

        public InconsistentTopicStatus(int totalCount, int totalCountChange)
        {
            TotalCount = totalCount;
            TotalCountChange = totalCountChange;
        }
    }

    /// <summary>Status when a sample is rejected by a DataReader.</summary>
    public class SampleRejectedStatus
    {
        public int TotalCount { get; }
        public int TotalCountChange { get; }
        public int LastReason { get; }

        public SampleRejectedStatus(int totalCount, int totalCountChange, int lastReason)
        {
            TotalCount = totalCount;
            TotalCountChange = totalCountChange;
            LastReason = lastReason;
        }
    }

    /// <summary>Status when a DataReader detects incompatible QoS with a DataWriter.</summary>
    public class RequestedIncompatibleQosStatus
    {
        public int TotalCount { get; }
        public int TotalCountChange { get; }
        public int LastPolicyId { get; }

        public RequestedIncompatibleQosStatus(int totalCount, int totalCountChange, int lastPolicyId)
        {
            TotalCount = totalCount;
            TotalCountChange = totalCountChange;
            LastPolicyId = lastPolicyId;
        }
    }

    /// <summary>Status when a DataWriter detects incompatible QoS with a DataReader.</summary>
    public class OfferedIncompatibleQosStatus
    {
        public int TotalCount { get; }
        public int TotalCountChange { get; }
        public int LastPolicyId { get; }

        public OfferedIncompatibleQosStatus(int totalCount, int totalCountChange, int lastPolicyId)
        {
            TotalCount = totalCount;
            TotalCountChange = totalCountChange;
            LastPolicyId = lastPolicyId;
        }
    }

    /// <summary>Status when a DataReader detects a remote DataWriter with an incompatible type.</summary>
    public class RequestedIncompatibleTypeStatus
    {
        public int TotalCount { get; }
        public int TotalCountChange { get; }

        public RequestedIncompatibleTypeStatus(int totalCount, int totalCountChange)
        {
            TotalCount = totalCount;
            TotalCountChange = totalCountChange;
        }
    }

    /// <summary>Status when a DataWriter detects a remote DataReader with an incompatible type.</summary>
    public class OfferedIncompatibleTypeStatus
    {
        public int TotalCount { get; }
        public int TotalCountChange { get; }

        public OfferedIncompatibleTypeStatus(int totalCount, int totalCountChange)
        {
            TotalCount = totalCount;
            TotalCountChange = totalCountChange;
        }
    }
}
