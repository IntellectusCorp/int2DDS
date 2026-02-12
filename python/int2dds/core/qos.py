"""
QoS policy classes for DDS entities.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import IntEnum
from typing import Literal


class ReliabilityKind(IntEnum):
    """Reliability QoS kind."""

    BEST_EFFORT = 0
    RELIABLE = 1


class DurabilityKind(IntEnum):
    """Durability QoS kind."""

    VOLATILE = 0
    TRANSIENT_LOCAL = 1
    TRANSIENT = 2
    PERSISTENT = 3


class HistoryKind(IntEnum):
    """History QoS kind."""

    KEEP_LAST = 0
    KEEP_ALL = 1


@dataclass
class Reliability:
    """
    Reliability QoS policy.

    Attributes:
        kind: BEST_EFFORT or RELIABLE
        max_blocking_time: Maximum time to block on write (seconds), only for RELIABLE
    """

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
    """
    Durability QoS policy.

    Attributes:
        kind: VOLATILE, TRANSIENT_LOCAL, TRANSIENT, or PERSISTENT
    """

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
    """
    History QoS policy.

    Attributes:
        kind: KEEP_LAST or KEEP_ALL
        depth: Number of samples to keep (only for KEEP_LAST)
    """

    kind: Literal["KEEP_LAST", "KEEP_ALL"] = "KEEP_LAST"
    depth: int = 1

    @property
    def _kind_int(self) -> int:
        return HistoryKind.KEEP_ALL if self.kind == "KEEP_ALL" else HistoryKind.KEEP_LAST


@dataclass
class DataWriterQos:
    """
    QoS settings for DataWriter.

    Example:
        >>> qos = DataWriterQos(
        ...     reliability=Reliability("RELIABLE"),
        ...     durability=Durability("TRANSIENT_LOCAL"),
        ... )
    """

    reliability: Reliability = field(default_factory=lambda: Reliability("RELIABLE"))
    durability: Durability = field(default_factory=lambda: Durability("VOLATILE"))
    history: History = field(default_factory=History)


@dataclass
class DataReaderQos:
    """
    QoS settings for DataReader.

    Example:
        >>> qos = DataReaderQos(
        ...     reliability=Reliability("BEST_EFFORT"),
        ...     history=History("KEEP_ALL"),
        ... )
    """

    reliability: Reliability = field(default_factory=lambda: Reliability("BEST_EFFORT"))
    durability: Durability = field(default_factory=lambda: Durability("VOLATILE"))
    history: History = field(default_factory=History)


@dataclass
class TopicQos:
    """QoS settings for Topic."""

    pass  # Reserved for future use
