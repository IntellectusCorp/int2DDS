//! QoS compatibility checking for reader-writer matching.
//!
//! This module provides functions to check QoS compatibility between DataReaders
//! and DataWriters. Incompatible QoS policies prevent communication between endpoints.

use crate::{
    common::builtin::topic::{
        publication_builtin_topic_data::PublicationBuiltinTopicData,
        subscription_builtin_topic_data::SubscriptionBuiltinTopicData,
    },
    infrastructure::qos_policy::{DataRepresentationQosPolicy, QosPolicyId},
};

pub(crate) fn check_qos_compatibility(
    requested: &SubscriptionBuiltinTopicData,
    offered: &PublicationBuiltinTopicData,
) -> bool {
    if offered.durability().kind < requested.durability().kind
        || offered.deadline().period > requested.deadline().period
        || offered.ownership().kind != requested.ownership().kind
        || offered.reliability().kind < requested.reliability().kind
        || offered.liveliness().kind < requested.liveliness().kind
        || offered.liveliness().lease_duration > requested.liveliness().lease_duration
        || offered.presentation().access_scope < requested.presentation().access_scope
        || (!offered.presentation().coherent_access && requested.presentation().coherent_access)
        || (!offered.presentation().ordered_access && requested.presentation().ordered_access)
        || offered.destination_order().kind < requested.destination_order().kind
        || offered.latency_budget().duration > requested.latency_budget().duration
        || offered.type_name() != requested.type_name()
        || !is_data_representation_compatible(
            requested.data_representation(),
            offered.data_representation(),
        )
    {
        return false;
    }

    true
}

pub(crate) fn is_data_representation_compatible(
    requested: &DataRepresentationQosPolicy,
    offered: &DataRepresentationQosPolicy,
) -> bool {
    // Empty list resolves to the default representation (single source).
    let requested_reps = requested.effective_ids();
    let offered_reps = offered.effective_ids();

    // DDS-XTypes spec 7.6.3.4.1:
    // DataRepresentation QoS is compatible if the intersection of Writer and Reader is not empty
    for offered_id in offered_reps {
        if requested_reps.contains(offered_id) {
            return true;
        }
    }
    false
}

pub(crate) fn check_qos_compatibility_with_policy_id(
    requested: &SubscriptionBuiltinTopicData,
    offered: &PublicationBuiltinTopicData,
) -> Option<QosPolicyId> {
    if offered.durability().kind < requested.durability().kind {
        return Some(QosPolicyId::Durability);
    }
    if offered.deadline().period > requested.deadline().period {
        return Some(QosPolicyId::Deadline);
    }
    if offered.ownership().kind != requested.ownership().kind {
        return Some(QosPolicyId::Ownership);
    }

    if offered.reliability().kind < requested.reliability().kind {
        return Some(QosPolicyId::Reliability);
    }

    if offered.liveliness().kind < requested.liveliness().kind {
        return Some(QosPolicyId::Liveliness);
    }
    if offered.liveliness().lease_duration > requested.liveliness().lease_duration {
        return Some(QosPolicyId::Liveliness);
    }
    if offered.presentation().access_scope < requested.presentation().access_scope {
        return Some(QosPolicyId::Presentation);
    }
    if !offered.presentation().coherent_access && requested.presentation().coherent_access {
        return Some(QosPolicyId::Presentation);
    }
    if !offered.presentation().ordered_access && requested.presentation().ordered_access {
        return Some(QosPolicyId::Presentation);
    }
    if offered.destination_order().kind < requested.destination_order().kind {
        return Some(QosPolicyId::DestinationOrder);
    }
    if offered.latency_budget().duration > requested.latency_budget().duration {
        return Some(QosPolicyId::LatencyBudget);
    }
    if !is_data_representation_compatible(
        requested.data_representation(),
        offered.data_representation(),
    ) {
        return Some(QosPolicyId::DataRepresentation);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::builtin::topic::{
        publication_builtin_topic_data::PublicationBuiltinTopicData,
        subscription_builtin_topic_data::SubscriptionBuiltinTopicData,
    };
    use crate::core::time::Duration;
    use crate::dcps::infrastructure::qos_policy::{
        DataRepresentationId, DeadlineQosPolicy, DestinationOrderQosPolicyKind,
        DurabilityQosPolicyKind, LivelinessQosPolicy, LivelinessQosPolicyKind, OwnershipQosPolicy,
        OwnershipQosPolicyKind, PresentationQosAccessScopeKind, PresentationQosPolicy,
        ReliabilityQosPolicy, ReliabilityQosPolicyKind,
    };
    use crate::infrastructure::qos_policy::DurabilityQosPolicy;
    use crate::publication::qos::{DataWriterQos, PublisherQos};
    use crate::subscription::qos::{DataReaderQos, SubscriberQos};
    use crate::topic::qos::TopicQos;

    #[test]
    fn test_default_should_be_compatible() {
        let requested = SubscriptionBuiltinTopicData::new(
            &DataReaderQos::default(),
            &SubscriberQos::default(),
            &TopicQos::default(),
        );
        let offered = PublicationBuiltinTopicData::new(
            &DataWriterQos::default(),
            &PublisherQos::default(),
            &TopicQos::default(),
        );

        assert!(check_qos_compatibility(&requested, &offered));
    }

    #[test]
    fn test_durability_kind_incompatible() {
        let requested = SubscriptionBuiltinTopicData::new(
            &DataReaderQos {
                durability: DurabilityQosPolicy { kind: DurabilityQosPolicyKind::TransientLocal },
                ..Default::default()
            },
            &SubscriberQos::default(),
            &TopicQos::default(),
        );

        let offered = PublicationBuiltinTopicData::new(
            &DataWriterQos {
                durability: DurabilityQosPolicy { kind: DurabilityQosPolicyKind::Volatile },
                ..Default::default()
            },
            &PublisherQos::default(),
            &TopicQos::default(),
        );

        assert!(!check_qos_compatibility(&requested, &offered));
    }

    #[test]
    fn test_deadline_period_incompatible() {
        let requested = SubscriptionBuiltinTopicData::new(
            &DataReaderQos {
                deadline: DeadlineQosPolicy { period: Duration { sec: 5, nanosec: 0 } },
                ..Default::default()
            },
            &SubscriberQos::default(),
            &TopicQos::default(),
        );
        let offered = PublicationBuiltinTopicData::new(
            &DataWriterQos {
                deadline: DeadlineQosPolicy { period: Duration { sec: 10, nanosec: 0 } },
                ..Default::default()
            },
            &PublisherQos::default(),
            &TopicQos::default(),
        );

        assert!(!check_qos_compatibility(&requested, &offered));
    }

    #[test]
    fn test_deadline_period_compatible() {
        let requested = SubscriptionBuiltinTopicData::new(
            &DataReaderQos {
                deadline: DeadlineQosPolicy { period: Duration { sec: 5, nanosec: 0 } },
                ..Default::default()
            },
            &SubscriberQos::default(),
            &TopicQos::default(),
        );
        let offered = PublicationBuiltinTopicData::new(
            &DataWriterQos {
                deadline: DeadlineQosPolicy { period: Duration { sec: 5, nanosec: 0 } },
                ..Default::default()
            },
            &PublisherQos::default(),
            &TopicQos::default(),
        );

        assert!(check_qos_compatibility(&requested, &offered));
    }

    #[test]
    fn test_ownership_kind_incompatible() {
        let requested = SubscriptionBuiltinTopicData::new(
            &DataReaderQos {
                ownership: OwnershipQosPolicy { kind: OwnershipQosPolicyKind::Exclusive },
                ..Default::default()
            },
            &SubscriberQos::default(),
            &TopicQos::default(),
        );

        let offered = PublicationBuiltinTopicData::new(
            &DataWriterQos {
                ownership: OwnershipQosPolicy { kind: OwnershipQosPolicyKind::Shared },
                ..Default::default()
            },
            &PublisherQos::default(),
            &TopicQos::default(),
        );

        assert!(!check_qos_compatibility(&requested, &offered));
    }

    #[test]
    fn test_reliability_kind_incompatible() {
        let requested = SubscriptionBuiltinTopicData::new(
            &DataReaderQos {
                reliability: ReliabilityQosPolicy {
                    kind: ReliabilityQosPolicyKind::Reliable,
                    max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
                },
                ..Default::default()
            },
            &SubscriberQos::default(),
            &TopicQos::default(),
        );

        let offered = PublicationBuiltinTopicData::new(
            &DataWriterQos {
                reliability: ReliabilityQosPolicy {
                    kind: ReliabilityQosPolicyKind::BestEffort,
                    max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
                },
                ..Default::default()
            },
            &PublisherQos::default(),
            &TopicQos::default(),
        );

        assert!(!check_qos_compatibility(&requested, &offered));
    }

    #[test]
    fn test_liveliness_kind_incompatible() {
        let requested = SubscriptionBuiltinTopicData::new(
            &DataReaderQos {
                liveliness: LivelinessQosPolicy {
                    kind: LivelinessQosPolicyKind::ManualByTopic,
                    ..Default::default()
                },
                ..Default::default()
            },
            &SubscriberQos::default(),
            &TopicQos::default(),
        );

        let offered = PublicationBuiltinTopicData::new(
            &DataWriterQos {
                liveliness: LivelinessQosPolicy {
                    kind: LivelinessQosPolicyKind::Automatic,
                    ..Default::default()
                },
                ..Default::default()
            },
            &PublisherQos::default(),
            &TopicQos::default(),
        );

        assert!(!check_qos_compatibility(&requested, &offered));
    }

    #[test]
    fn test_liveliness_lease_duration_incompatible() {
        let requested = SubscriptionBuiltinTopicData::new(
            &DataReaderQos {
                liveliness: LivelinessQosPolicy {
                    lease_duration: Duration { sec: 5, nanosec: 0 },
                    ..Default::default()
                },
                ..Default::default()
            },
            &SubscriberQos::default(),
            &TopicQos::default(),
        );

        let offered = PublicationBuiltinTopicData::new(
            &DataWriterQos {
                liveliness: LivelinessQosPolicy {
                    lease_duration: Duration { sec: 10, nanosec: 0 },
                    ..Default::default()
                },
                ..Default::default()
            },
            &PublisherQos::default(),
            &TopicQos::default(),
        );

        assert!(!check_qos_compatibility(&requested, &offered));
    }

    #[test]
    fn test_presentation_access_scope_incompatible() {
        let requested = SubscriptionBuiltinTopicData::new(
            &DataReaderQos::default(),
            &SubscriberQos {
                presentation: PresentationQosPolicy {
                    access_scope: PresentationQosAccessScopeKind::Group,
                    ..Default::default()
                },
                ..Default::default()
            },
            &TopicQos::default(),
        );

        let offered = PublicationBuiltinTopicData::new(
            &DataWriterQos::default(),
            &PublisherQos {
                presentation: PresentationQosPolicy {
                    access_scope: PresentationQosAccessScopeKind::Instance,
                    ..Default::default()
                },
                ..Default::default()
            },
            &TopicQos::default(),
        );

        assert!(!check_qos_compatibility(&requested, &offered));
    }

    #[test]
    fn test_presentation_coherent_access_incompatible() {
        let requested = SubscriptionBuiltinTopicData::new(
            &DataReaderQos::default(),
            &SubscriberQos {
                presentation: PresentationQosPolicy { coherent_access: true, ..Default::default() },
                ..Default::default()
            },
            &TopicQos::default(),
        );
        let offered = PublicationBuiltinTopicData::new(
            &DataWriterQos::default(),
            &PublisherQos {
                presentation: PresentationQosPolicy {
                    coherent_access: false,
                    ..Default::default()
                },
                ..Default::default()
            },
            &TopicQos::default(),
        );
        assert!(!check_qos_compatibility(&requested, &offered));
    }

    #[test]
    fn test_presentation_ordered_access_incompatible() {
        let requested = SubscriptionBuiltinTopicData::new(
            &DataReaderQos::default(),
            &SubscriberQos {
                presentation: PresentationQosPolicy { ordered_access: true, ..Default::default() },
                ..Default::default()
            },
            &TopicQos::default(),
        );
        let offered = PublicationBuiltinTopicData::new(
            &DataWriterQos::default(),
            &PublisherQos {
                presentation: PresentationQosPolicy { ordered_access: false, ..Default::default() },
                ..Default::default()
            },
            &TopicQos::default(),
        );
        assert!(!check_qos_compatibility(&requested, &offered));
    }

    #[test]
    fn test_destination_order_kind_incompatible() {
        let requested = SubscriptionBuiltinTopicData::new(
            &DataReaderQos {
                destination_order:
                    crate::dcps::infrastructure::qos_policy::DestinationOrderQosPolicy {
                        kind: DestinationOrderQosPolicyKind::BySourceTimestamp,
                    },
                ..Default::default()
            },
            &SubscriberQos::default(),
            &TopicQos::default(),
        );

        let offered = PublicationBuiltinTopicData::new(
            &DataWriterQos {
                destination_order:
                    crate::dcps::infrastructure::qos_policy::DestinationOrderQosPolicy {
                        kind: DestinationOrderQosPolicyKind::ByReceptionTimestamp,
                    },
                ..Default::default()
            },
            &PublisherQos::default(),
            &TopicQos::default(),
        );

        assert!(!check_qos_compatibility(&requested, &offered));
    }

    #[test]
    fn test_latency_budget_duration_incompatible() {
        let requested = SubscriptionBuiltinTopicData::new(
            &DataReaderQos {
                latency_budget: crate::dcps::infrastructure::qos_policy::LatencyBudgetQosPolicy {
                    duration: Duration { sec: 5, nanosec: 0 },
                },
                ..Default::default()
            },
            &SubscriberQos::default(),
            &TopicQos::default(),
        );

        let offered = PublicationBuiltinTopicData::new(
            &DataWriterQos {
                latency_budget: crate::dcps::infrastructure::qos_policy::LatencyBudgetQosPolicy {
                    duration: Duration { sec: 10, nanosec: 0 },
                },
                ..Default::default()
            },
            &PublisherQos::default(),
            &TopicQos::default(),
        );

        assert!(!check_qos_compatibility(&requested, &offered));
    }

    #[test]
    fn test_data_representation_compatible() {
        let requested = SubscriptionBuiltinTopicData::new(
            &DataReaderQos {
                data_representation:
                    crate::dcps::infrastructure::qos_policy::DataRepresentationQosPolicy {
                        value: vec![
                            DataRepresentationId::XcdrDataRepresentation,
                            DataRepresentationId::Xcdr2DataRepresentation,
                        ],
                    },
                ..Default::default()
            },
            &SubscriberQos::default(),
            &TopicQos::default(),
        );

        let offered = PublicationBuiltinTopicData::new(
            &DataWriterQos {
                data_representation:
                    crate::dcps::infrastructure::qos_policy::DataRepresentationQosPolicy {
                        value: vec![DataRepresentationId::XcdrDataRepresentation],
                    },
                ..Default::default()
            },
            &PublisherQos::default(),
            &TopicQos::default(),
        );

        let requested_2 = SubscriptionBuiltinTopicData::new(
            &DataReaderQos {
                data_representation:
                    crate::dcps::infrastructure::qos_policy::DataRepresentationQosPolicy {
                        value: vec![
                            DataRepresentationId::XcdrDataRepresentation,
                            DataRepresentationId::Xcdr2DataRepresentation,
                        ],
                    },
                ..Default::default()
            },
            &SubscriberQos::default(),
            &TopicQos::default(),
        );

        let offered_2 = PublicationBuiltinTopicData::new(
            &DataWriterQos {
                data_representation:
                    crate::dcps::infrastructure::qos_policy::DataRepresentationQosPolicy {
                        value: vec![DataRepresentationId::Xcdr2DataRepresentation],
                    },
                ..Default::default()
            },
            &PublisherQos::default(),
            &TopicQos::default(),
        );

        let requested_3 = SubscriptionBuiltinTopicData::new(
            &DataReaderQos {
                data_representation:
                    crate::dcps::infrastructure::qos_policy::DataRepresentationQosPolicy {
                        value: vec![DataRepresentationId::XcdrDataRepresentation],
                    },
                ..Default::default()
            },
            &SubscriberQos::default(),
            &TopicQos::default(),
        );

        let offered_3 = PublicationBuiltinTopicData::new(
            &DataWriterQos {
                data_representation:
                    crate::dcps::infrastructure::qos_policy::DataRepresentationQosPolicy {
                        value: vec![DataRepresentationId::XcdrDataRepresentation],
                    },
                ..Default::default()
            },
            &PublisherQos::default(),
            &TopicQos::default(),
        );

        assert!(check_qos_compatibility(&requested, &offered));
        assert!(check_qos_compatibility(&requested_2, &offered_2));
        assert!(check_qos_compatibility(&requested_3, &offered_3));
    }

    #[test]
    fn test_data_representation_incompatible() {
        let requested = SubscriptionBuiltinTopicData::new(
            &DataReaderQos {
                data_representation:
                    crate::dcps::infrastructure::qos_policy::DataRepresentationQosPolicy {
                        value: vec![DataRepresentationId::XcdrDataRepresentation],
                    },
                ..Default::default()
            },
            &SubscriberQos::default(),
            &TopicQos::default(),
        );

        let offered = PublicationBuiltinTopicData::new(
            &DataWriterQos {
                data_representation:
                    crate::dcps::infrastructure::qos_policy::DataRepresentationQosPolicy {
                        value: vec![DataRepresentationId::Xcdr2DataRepresentation],
                    },
                ..Default::default()
            },
            &PublisherQos::default(),
            &TopicQos::default(),
        );

        assert!(!check_qos_compatibility(&requested, &offered));
    }
}
