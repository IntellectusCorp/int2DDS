"""
int2dds - Python bindings for int2DDS

A high-performance DDS (Data Distribution Service) implementation.
"""

try:
    from int2dds.core.participant import DomainParticipant
    from int2dds.core.publisher import DataWriter, Publisher
    from int2dds.core.subscriber import DataReader, Sample, Subscriber
    from int2dds.core.topic import ContentFilteredTopic, Topic
    from int2dds.core.qos import (
        DataReaderQos,
        DataWriterQos,
        DataRepresentation,
        Deadline,
        DestinationOrder,
        Durability,
        History,
        LatencyBudget,
        Lifespan,
        Liveliness,
        Ownership,
        OwnershipStrength,
        Partition,
        ParticipantQos,
        Property,
        PublisherQos,
        ReaderDataLifecycle,
        Reliability,
        ResourceLimits,
        SubscriberQos,
        TimeBasedFilter,
        TopicQos,
        TransportPriority,
        UserData,
        WriterDataLifecycle,
    )
    from int2dds.core.conditions import GuardCondition, StatusCondition, WaitSet
    from int2dds.core.async_support import AsyncDataReader, AsyncWaitSet, async_wait
    from int2dds.core.listeners import (
        DataReaderListener,
        DataReaderListenerBase,
        DataWriterListener,
        DataWriterListenerBase,
        LivelinessChangedStatus,
        LivelinessLostStatus,
        OfferedDeadlineMissedStatus,
        OfferedIncompatibleQosStatus,
        PublicationMatchedStatus,
        RequestedDeadlineMissedStatus,
        RequestedIncompatibleQosStatus,
        SampleLostStatus,
        SampleRejectedStatus,
        SubscriptionMatchedStatus,
    )
    from int2dds.exceptions import (
        DdsError,
        DdsInvalidArgument,
        DdsNoData,
        DdsPreconditionNotMet,
        DdsTimeout,
    )
    from int2dds.config import (
        ConfiguredParticipant,
        create_participant_from_config,
        get_dynamic_type_support,
        load_profiles,
    )
    from int2dds import env
except ImportError as e:
    import warnings
    warnings.warn(f"int2dds core modules not available: {e}")

__version__ = "0.1.0"
__all__ = [
    # Core entities
    "DomainParticipant",
    "Publisher",
    "Subscriber",
    "DataWriter",
    "DataReader",
    "Topic",
    "ContentFilteredTopic",
    "Sample",
    # QoS
    "DataWriterQos",
    "DataReaderQos",
    "Reliability",
    "Durability",
    "History",
    # Conditions
    "WaitSet",
    "Condition",
    "StatusCondition",
    "GuardCondition",
    # Async support
    "AsyncWaitSet",
    "AsyncDataReader",
    "async_wait",
    # Listeners
    "DataWriterListener",
    "DataWriterListenerBase",
    "DataReaderListener",
    "DataReaderListenerBase",
    "PublicationMatchedStatus",
    "SubscriptionMatchedStatus",
    "OfferedDeadlineMissedStatus",
    "RequestedDeadlineMissedStatus",
    "LivelinessLostStatus",
    "LivelinessChangedStatus",
    "SampleLostStatus",
    "SampleRejectedStatus",
    "RequestedIncompatibleQosStatus",
    "OfferedIncompatibleQosStatus",
    # Exceptions
    # Exceptions
    "DdsError",
    "DdsTimeout",
    "DdsNoData",
    "DdsInvalidArgument",
    "DdsPreconditionNotMet",
    # XML configuration (QoS profiles + participant tree)
    "load_profiles",
    "get_dynamic_type_support",
    "create_participant_from_config",
    "ConfiguredParticipant",
    # Environment configuration
    "env",
]
