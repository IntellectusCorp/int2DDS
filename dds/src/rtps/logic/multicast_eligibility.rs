#![allow(dead_code)]

use crate::{
    common::builtin::topic::subscription_builtin_topic_data::SubscriptionBuiltinTopicData,
    rtps::{common::locator::Locator, entities::history::cache_change::CacheChange},
};

/// Whether a matched remote Reader can be served over multicast.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ReaderMulticastVerdict {
    Eligible { group_locators: Vec<Locator> },
    Ineligible,
}

pub(crate) fn evaluate_reader_multicast(
    subscription: &SubscriptionBuiltinTopicData,
    has_content_filter: bool,
    is_reachable: impl Fn(&Locator) -> bool,
) -> ReaderMulticastVerdict {
    if has_content_filter {
        return ReaderMulticastVerdict::Ineligible;
    }

    let group_locators: Vec<Locator> = subscription
        .multicast_locator_list()
        .into_iter()
        .filter(|locator| is_reachable(locator))
        .collect();

    if group_locators.is_empty() {
        return ReaderMulticastVerdict::Ineligible;
    }

    ReaderMulticastVerdict::Eligible { group_locators }
}

pub(crate) enum MulticastSendType {
    FirstSample,
    ReSendSample,
}

pub(crate) fn sample_allows_multicast(change: &CacheChange, send_type: MulticastSendType) -> bool {
    match send_type {
        MulticastSendType::FirstSample => {
            let presentation_info = change.presentation_info();
            let in_coherent_set = presentation_info.coherent_set.is_some()
                || presentation_info.group_coherent_set.is_some()
                || change.is_coherent_end_marker();

            !change.is_fragmented() && !in_coherent_set
        }
        MulticastSendType::ReSendSample => false,
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use super::*;
    use crate::{
        common::instance_handle::InstanceHandle,
        infrastructure::qos_policy::{DurabilityQosPolicyKind, ReliabilityQosPolicyKind},
        rtps::{
            common::{
                entity_id::EntityId, entity_kind::EntityKind, guid::Guid, sequence::SequenceNumber,
                types::ChangeKind,
            },
            entities::history::cache_change::PresentationInfo,
        },
        subscription::qos::{DataReaderQos, SubscriberQos},
        topic::qos::TopicQos,
    };

    const USER_TRAFFIC_MULTICAST_PORT: u32 = 7401;

    fn group_locator(last_octet: u8) -> Locator {
        Locator::from_ip_v4_addr_and_port(
            &Ipv4Addr::new(239, 255, 0, last_octet),
            USER_TRAFFIC_MULTICAST_PORT,
        )
    }

    fn subscription_with_groups(groups: &[Locator]) -> SubscriptionBuiltinTopicData {
        let mut subscription = SubscriptionBuiltinTopicData::default();
        for group in groups {
            subscription.add_multicast_locator(group.clone());
        }
        subscription
    }

    fn writer_guid() -> Guid {
        Guid::new([1; 12], EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY))
    }

    fn plain_change() -> CacheChange {
        CacheChange::new(
            ChangeKind::Alive,
            writer_guid(),
            InstanceHandle::default(),
            SequenceNumber::from_i64(1),
            vec![0; 16],
            None,
        )
    }

    #[test]
    fn reader_without_group_is_ineligible() {
        let subscription = subscription_with_groups(&[]);

        assert_eq!(
            evaluate_reader_multicast(&subscription, false, |_| true),
            ReaderMulticastVerdict::Ineligible
        );
    }

    #[test]
    fn reader_with_reachable_group_is_eligible() {
        let group = group_locator(1);
        let subscription = subscription_with_groups(&[group.clone()]);

        assert_eq!(
            evaluate_reader_multicast(&subscription, false, |_| true),
            ReaderMulticastVerdict::Eligible { group_locators: vec![group] }
        );
    }

    #[test]
    fn reader_whose_groups_are_all_unreachable_is_ineligible() {
        let subscription = subscription_with_groups(&[group_locator(1), group_locator(2)]);

        assert_eq!(
            evaluate_reader_multicast(&subscription, false, |_| false),
            ReaderMulticastVerdict::Ineligible
        );
    }

    #[test]
    fn unreachable_groups_are_dropped_from_the_verdict() {
        let reachable = group_locator(1);
        let unreachable = group_locator(2);
        let subscription = subscription_with_groups(&[reachable.clone(), unreachable.clone()]);

        assert_eq!(
            evaluate_reader_multicast(&subscription, false, |locator| locator != &unreachable),
            ReaderMulticastVerdict::Eligible { group_locators: vec![reachable] }
        );
    }

    #[test]
    fn content_filtered_reader_is_ineligible() {
        let subscription = subscription_with_groups(&[group_locator(1)]);

        assert_eq!(
            evaluate_reader_multicast(&subscription, true, |_| true),
            ReaderMulticastVerdict::Ineligible
        );
    }

    #[test]
    fn best_effort_transient_local_reader_stays_eligible() {
        let mut reader_qos = DataReaderQos::default();
        reader_qos.reliability.kind = ReliabilityQosPolicyKind::BestEffort;
        reader_qos.durability.kind = DurabilityQosPolicyKind::TransientLocal;

        let group = group_locator(1);
        let mut subscription = SubscriptionBuiltinTopicData::new(
            &reader_qos,
            &SubscriberQos::default(),
            &TopicQos::default(),
        );
        subscription.add_multicast_locator(group.clone());

        assert_eq!(
            evaluate_reader_multicast(&subscription, false, |_| true),
            ReaderMulticastVerdict::Eligible { group_locators: vec![group] }
        );
    }

    #[test]
    fn plain_first_sample_allows_multicast() {
        assert!(sample_allows_multicast(&plain_change(), MulticastSendType::FirstSample));
    }

    #[test]
    fn fragmented_sample_is_denied() {
        let mut change = plain_change();
        change.apply_fragmentation(4);

        assert!(!sample_allows_multicast(&change, MulticastSendType::FirstSample));
    }

    #[test]
    fn coherent_set_member_is_denied() {
        let mut change = plain_change();
        change.set_presentation_info(PresentationInfo {
            coherent_set: Some(SequenceNumber::from_i64(1)),
            ..Default::default()
        });

        assert!(!sample_allows_multicast(&change, MulticastSendType::FirstSample));
    }

    #[test]
    fn group_coherent_set_member_is_denied() {
        let mut change = plain_change();
        change.set_presentation_info(PresentationInfo {
            group_coherent_set: Some(SequenceNumber::from_i64(1)),
            ..Default::default()
        });

        assert!(!sample_allows_multicast(&change, MulticastSendType::FirstSample));
    }

    #[test]
    fn coherent_end_marker_is_denied() {
        let marker = CacheChange::new(
            ChangeKind::Alive,
            writer_guid(),
            InstanceHandle::default(),
            SequenceNumber::from_i64(2),
            Vec::new(),
            None,
        );

        assert!(!sample_allows_multicast(&marker, MulticastSendType::FirstSample));
    }

    #[test]
    fn resent_sample_is_denied() {
        let change = plain_change();

        assert!(sample_allows_multicast(&change, MulticastSendType::FirstSample));
        assert!(!sample_allows_multicast(&change, MulticastSendType::ReSendSample));
    }
}
