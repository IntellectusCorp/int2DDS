"""
QoS policy classes for DDS entities.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import IntEnum
from typing import Literal


class ReliabilityKind(IntEnum):
    BEST_EFFORT = 0
    RELIABLE = 1


class DurabilityKind(IntEnum):
    VOLATILE = 0
    TRANSIENT_LOCAL = 1
    TRANSIENT = 2
    PERSISTENT = 3


class HistoryKind(IntEnum):
    KEEP_LAST = 0
    KEEP_ALL = 1


class OwnershipKind(IntEnum):
    SHARED = 0
    EXCLUSIVE = 1


class DestinationOrderKind(IntEnum):
    BY_RECEPTION = 0
    BY_SOURCE = 1


class LivelinessKind(IntEnum):
    AUTOMATIC = 0
    MANUAL_BY_PARTICIPANT = 1
    MANUAL_BY_TOPIC = 2


class DataRepresentationKind(IntEnum):
    XCDR1 = 0
    XCDR2 = 2


# ---------------------------------------------------------------------------
# QoS Policy dataclasses
# ---------------------------------------------------------------------------

@dataclass
class Reliability:
    kind: Literal["BEST_EFFORT", "RELIABLE"] = "RELIABLE"
    max_blocking_time: float = 0.1  # seconds

    @property
    def _kind_int(self) -> int:
        return ReliabilityKind.RELIABLE if self.kind == "RELIABLE" else ReliabilityKind.BEST_EFFORT

    @property
    def _max_blocking_time_ns(self) -> int:
        return int(self.max_blocking_time * 1_000_000_000)


@dataclass
class Durability:
    kind: Literal["VOLATILE", "TRANSIENT_LOCAL", "TRANSIENT", "PERSISTENT"] = "VOLATILE"

    @property
    def _kind_int(self) -> int:
        mapping = {
            "VOLATILE": DurabilityKind.VOLATILE,
            "TRANSIENT_LOCAL": DurabilityKind.TRANSIENT_LOCAL,
            "TRANSIENT": DurabilityKind.TRANSIENT,
            "PERSISTENT": DurabilityKind.PERSISTENT,
        }
        return mapping[self.kind]


@dataclass
class History:
    kind: Literal["KEEP_LAST", "KEEP_ALL"] = "KEEP_LAST"
    depth: int = 1

    @property
    def _kind_int(self) -> int:
        return HistoryKind.KEEP_ALL if self.kind == "KEEP_ALL" else HistoryKind.KEEP_LAST


@dataclass
class Ownership:
    kind: Literal["SHARED", "EXCLUSIVE"] = "SHARED"

    @property
    def _kind_int(self) -> int:
        return OwnershipKind.EXCLUSIVE if self.kind == "EXCLUSIVE" else OwnershipKind.SHARED


@dataclass
class OwnershipStrength:
    value: int = 0


@dataclass
class ResourceLimits:
    max_samples: int = -1  # -1 = unlimited
    max_instances: int = -1
    max_samples_per_instance: int = -1


@dataclass
class Lifespan:
    duration: float = float("inf")  # seconds, inf = infinite

    @property
    def _duration_ns(self) -> int:
        if self.duration == float("inf"):
            return 0x7FFFFFFFFFFFFFFF
        return int(self.duration * 1_000_000_000)


@dataclass
class DestinationOrder:
    kind: Literal["BY_RECEPTION", "BY_SOURCE"] = "BY_RECEPTION"

    @property
    def _kind_int(self) -> int:
        return DestinationOrderKind.BY_SOURCE if self.kind == "BY_SOURCE" else DestinationOrderKind.BY_RECEPTION


@dataclass
class LatencyBudget:
    duration: float = 0.0  # seconds

    @property
    def _duration_ns(self) -> int:
        return int(self.duration * 1_000_000_000)


@dataclass
class TransportPriority:
    value: int = 0


@dataclass
class UserData:
    data: bytes = b""


@dataclass
class WriterDataLifecycle:
    autodispose_unregistered_instances: bool = True


@dataclass
class ReaderDataLifecycle:
    autopurge_nowriter_samples_delay: float = float("inf")  # seconds
    autopurge_disposed_samples_delay: float = float("inf")

    @property
    def _autopurge_nowriter_ns(self) -> int:
        if self.autopurge_nowriter_samples_delay == float("inf"):
            return 0x7FFFFFFFFFFFFFFF
        return int(self.autopurge_nowriter_samples_delay * 1_000_000_000)

    @property
    def _autopurge_disposed_ns(self) -> int:
        if self.autopurge_disposed_samples_delay == float("inf"):
            return 0x7FFFFFFFFFFFFFFF
        return int(self.autopurge_disposed_samples_delay * 1_000_000_000)


@dataclass
class DataRepresentation:
    kind: Literal["XCDR1", "XCDR2"] = "XCDR2"

    @property
    def _kind_int(self) -> int:
        return DataRepresentationKind.XCDR2 if self.kind == "XCDR2" else DataRepresentationKind.XCDR1


@dataclass
class TimeBasedFilter:
    minimum_separation: float = 0.0  # seconds

    @property
    def _minimum_separation_ns(self) -> int:
        return int(self.minimum_separation * 1_000_000_000)


@dataclass
class Deadline:
    period: float = float("inf")  # seconds

    @property
    def _period_ns(self) -> int:
        if self.period == float("inf"):
            return 0x7FFFFFFFFFFFFFFF
        return int(self.period * 1_000_000_000)


@dataclass
class Liveliness:
    kind: Literal["AUTOMATIC", "MANUAL_BY_PARTICIPANT", "MANUAL_BY_TOPIC"] = "AUTOMATIC"
    lease_duration: float = float("inf")  # seconds

    @property
    def _kind_int(self) -> int:
        mapping = {
            "AUTOMATIC": LivelinessKind.AUTOMATIC,
            "MANUAL_BY_PARTICIPANT": LivelinessKind.MANUAL_BY_PARTICIPANT,
            "MANUAL_BY_TOPIC": LivelinessKind.MANUAL_BY_TOPIC,
        }
        return mapping[self.kind]

    @property
    def _lease_duration_ns(self) -> int:
        if self.lease_duration == float("inf"):
            return 0x7FFFFFFFFFFFFFFF
        return int(self.lease_duration * 1_000_000_000)


@dataclass
class Partition:
    names: list[str] = field(default_factory=list)


@dataclass
class Property:
    """PropertyQosPolicy entries (DomainParticipant only).

    Each entry is ``(name, value, propagate)``. The well-known name
    ``int2dds.transport.UDPv4.multicast_ttl`` configures the IPv4
    multicast TTL for the participant; use :meth:`set_multicast_ttl`
    as a convenience.
    """
    entries: list[tuple[str, str, bool]] = field(default_factory=list)

    def add(self, name: str, value: str, propagate: bool = True) -> None:
        self.entries.append((name, value, propagate))

    def set_multicast_ttl(self, ttl: int) -> None:
        if not 0 <= ttl <= 255:
            raise ValueError(f"multicast TTL must be in [0, 255], got {ttl}")
        # Replace any prior multicast TTL entry to match the Rust helper semantics.
        name = "int2dds.transport.UDPv4.multicast_ttl"
        self.entries = [e for e in self.entries if e[0] != name]
        self.entries.append((name, str(ttl), False))


# ---------------------------------------------------------------------------
# Composite QoS
# ---------------------------------------------------------------------------

@dataclass
class DataWriterQos:
    reliability: Reliability = field(default_factory=lambda: Reliability("RELIABLE"))
    durability: Durability = field(default_factory=lambda: Durability("VOLATILE"))
    history: History = field(default_factory=History)
    ownership: Ownership | None = None
    ownership_strength: OwnershipStrength | None = None
    resource_limits: ResourceLimits | None = None
    lifespan: Lifespan | None = None
    destination_order: DestinationOrder | None = None
    latency_budget: LatencyBudget | None = None
    transport_priority: TransportPriority | None = None
    user_data: UserData | None = None
    writer_data_lifecycle: WriterDataLifecycle | None = None
    data_representation: DataRepresentation | None = None
    deadline: Deadline | None = None
    liveliness: Liveliness | None = None


@dataclass
class DataReaderQos:
    reliability: Reliability = field(default_factory=lambda: Reliability("BEST_EFFORT"))
    durability: Durability = field(default_factory=lambda: Durability("VOLATILE"))
    history: History = field(default_factory=History)
    ownership: Ownership | None = None
    resource_limits: ResourceLimits | None = None
    destination_order: DestinationOrder | None = None
    time_based_filter: TimeBasedFilter | None = None
    latency_budget: LatencyBudget | None = None
    user_data: UserData | None = None
    reader_data_lifecycle: ReaderDataLifecycle | None = None
    data_representation: DataRepresentation | None = None
    deadline: Deadline | None = None
    liveliness: Liveliness | None = None


@dataclass
class TopicQos:
    reliability: Reliability | None = None
    durability: Durability | None = None
    history: History | None = None
    deadline: Deadline | None = None
    liveliness: Liveliness | None = None
    destination_order: DestinationOrder | None = None
    resource_limits: ResourceLimits | None = None
    transport_priority: TransportPriority | None = None
    lifespan: Lifespan | None = None
    ownership: Ownership | None = None
    data_representation: DataRepresentation | None = None


@dataclass
class PublisherQos:
    partition: Partition | None = None


@dataclass
class SubscriberQos:
    partition: Partition | None = None


@dataclass
class ParticipantQos:
    user_data: UserData | None = None
    property: Property | None = None
