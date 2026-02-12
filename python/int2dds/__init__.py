"""
int2dds - Python bindings for int2DDS

A high-performance DDS (Data Distribution Service) implementation.
"""

from int2dds.core.participant import DomainParticipant
from int2dds.core.publisher import DataWriter, Publisher
from int2dds.core.subscriber import DataReader, Sample, Subscriber
from int2dds.core.topic import Topic
from int2dds.core.qos import (
    DataReaderQos,
    DataWriterQos,
    Durability,
    History,
    Reliability,
)
from int2dds.core.conditions import GuardCondition, StatusCondition, WaitSet
from int2dds.exceptions import (
    DdsError,
    DdsInvalidArgument,
    DdsNoData,
    DdsPreconditionNotMet,
    DdsTimeout,
)

__version__ = "0.1.0"
__all__ = [
    # Core entities
    "DomainParticipant",
    "Publisher",
    "Subscriber",
    "DataWriter",
    "DataReader",
    "Topic",
    "Sample",
    # QoS
    "DataWriterQos",
    "DataReaderQos",
    "Reliability",
    "Durability",
    "History",
    # Conditions
    "WaitSet",
    "StatusCondition",
    "GuardCondition",
    # Exceptions
    "DdsError",
    "DdsTimeout",
    "DdsNoData",
    "DdsInvalidArgument",
    "DdsPreconditionNotMet",
]
