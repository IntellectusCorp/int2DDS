namespace Int2Dds.Qos;

public enum ReliabilityKind { BestEffort = 0, Reliable = 1 }
public enum DurabilityKind { Volatile = 0, TransientLocal = 1, Transient = 2, Persistent = 3 }
public enum HistoryKind { KeepLast = 0, KeepAll = 1 }
public enum OwnershipKind { Shared = 0, Exclusive = 1 }
public enum DestinationOrderKind { ByReception = 0, BySource = 1 }
public enum LivelinessKind { Automatic = 0, ManualByParticipant = 1, ManualByTopic = 2 }
public enum DataRepresentationKind { Xcdr1 = 0, Xcdr2 = 2 }
