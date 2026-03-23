namespace Int2Dds.Listeners;

/// <summary>Status of publication matching for a DataWriter.</summary>
public record PublicationMatchedStatus(int TotalCount, int TotalCountChange, int CurrentCount, int CurrentCountChange);

/// <summary>Status of subscription matching for a DataReader.</summary>
public record SubscriptionMatchedStatus(int TotalCount, int TotalCountChange, int CurrentCount, int CurrentCountChange);

/// <summary>Status when a DataWriter misses its offered deadline.</summary>
public record OfferedDeadlineMissedStatus(int TotalCount, int TotalCountChange);

/// <summary>Status when a DataReader misses a requested deadline.</summary>
public record RequestedDeadlineMissedStatus(int TotalCount, int TotalCountChange);

/// <summary>Status when a DataWriter loses liveliness.</summary>
public record LivelinessLostStatus(int TotalCount, int TotalCountChange);

/// <summary>Status when liveliness changes for a DataReader.</summary>
public record LivelinessChangedStatus(int AliveCount, int NotAliveCount, int AliveCountChange, int NotAliveCountChange);

/// <summary>Status when samples are lost.</summary>
public record SampleLostStatus(int TotalCount, int TotalCountChange);

/// <summary>Status when a sample is rejected by a DataReader.</summary>
public record SampleRejectedStatus(int TotalCount, int TotalCountChange, int LastReason);

/// <summary>Status when a DataReader detects incompatible QoS with a DataWriter.</summary>
public record RequestedIncompatibleQosStatus(int TotalCount, int TotalCountChange, int LastPolicyId);

/// <summary>Status when a DataWriter detects incompatible QoS with a DataReader.</summary>
public record OfferedIncompatibleQosStatus(int TotalCount, int TotalCountChange, int LastPolicyId);
