#![allow(dead_code)]

use log::{debug, trace};

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
        trace!("[Multicast] Reader {} not eligible: content filter", subscription.endpoint_guid());
        return ReaderMulticastVerdict::Ineligible;
    }

    let advertised_groups = subscription.multicast_locator_list();
    let group_locators: Vec<Locator> =
        advertised_groups.iter().filter(|locator| is_reachable(locator)).cloned().collect();

    if group_locators.is_empty() {
        trace!(
            "[Multicast] Reader {} not eligible: none of its {} advertised group(s) is reachable",
            subscription.endpoint_guid(),
            advertised_groups.len()
        );
        return ReaderMulticastVerdict::Ineligible;
    }

    trace!(
        "[Multicast] Reader {} eligible on {} of its {} advertised group(s)",
        subscription.endpoint_guid(),
        group_locators.len(),
        advertised_groups.len()
    );

    ReaderMulticastVerdict::Eligible { group_locators }
}

pub(crate) struct MulticastGroup<K> {
    pub(crate) locator: Locator,
    pub(crate) reader_list: Vec<K>,
}

pub(crate) struct MulticastGrouping<K> {
    pub(crate) multicast_groups: Vec<MulticastGroup<K>>,
    pub(crate) unicast_only_reader_list: Vec<K>,
}

/// Split send targets into multicast groups and unicast-only targets.
pub(crate) fn group_targets_by_multicast<'a, K, I>(
    targets: I,
    is_reachable: impl Fn(&Locator) -> bool,
) -> MulticastGrouping<K>
where
    I: IntoIterator<Item = (K, &'a SubscriptionBuiltinTopicData, bool)>,
{
    let mut groups: Vec<MulticastGroup<K>> = Vec::new();
    let mut unicast_only: Vec<K> = Vec::new();

    for (target, subscription, has_content_filter) in targets {
        // A Reader advertising several groups is served by its first reachable one: one copy
        // reaches it either way, and one key per Reader keeps the groups from splitting.
        let group_locator =
            match evaluate_reader_multicast(subscription, has_content_filter, &is_reachable) {
                ReaderMulticastVerdict::Eligible { group_locators } => {
                    group_locators.into_iter().next()
                }
                ReaderMulticastVerdict::Ineligible => None,
            };

        let Some(locator) = group_locator else {
            unicast_only.push(target);
            continue;
        };

        match groups.iter_mut().find(|group| group.locator == locator) {
            Some(group) => group.reader_list.push(target),
            None => groups.push(MulticastGroup { locator, reader_list: vec![target] }),
        }
    }

    // Silent while nothing is grouped, so a deployment without multicast stays quiet.
    if !groups.is_empty() {
        debug!(
            "[Multicast] {} group(s) formed, {} target(s) stay unicast-only",
            groups.len(),
            unicast_only.len()
        );
        for group in &groups {
            debug!(
                "[Multicast] Group {} serves {} target(s)",
                group.locator,
                group.reader_list.len()
            );
        }
    }

    MulticastGrouping { multicast_groups: groups, unicast_only_reader_list: unicast_only }
}

pub(crate) enum MulticastSendType {
    FirstSample,
    ReSendSample,
}

pub(crate) fn sample_allows_multicast(change: &CacheChange, send_type: MulticastSendType) -> bool {
    match send_type {
        MulticastSendType::FirstSample => {
            if change.is_fragmented() {
                trace!(
                    "[Multicast] SN {} not eligible: fragmented into {} piece(s)",
                    change.sequence_number(),
                    change.total_fragments()
                );
                return false;
            }

            let presentation_info = change.presentation_info();
            let in_coherent_set = presentation_info.coherent_set.is_some()
                || presentation_info.group_coherent_set.is_some()
                || change.is_coherent_end_marker();

            if in_coherent_set {
                trace!(
                    "[Multicast] SN {} not eligible: belongs to a coherent set",
                    change.sequence_number()
                );
                return false;
            }

            true
        }
        MulticastSendType::ReSendSample => {
            trace!("[Multicast] SN {} not eligible: resend", change.sequence_number());
            false
        }
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
    fn targets_sharing_a_group_land_in_one_group() {
        let group = group_locator(1);
        let subscription = subscription_with_groups(&[group.clone()]);

        let grouping = group_targets_by_multicast(
            [("a", &subscription, false), ("b", &subscription, false)],
            |_| true,
        );

        assert!(grouping.unicast_only_reader_list.is_empty());
        assert_eq!(grouping.multicast_groups.len(), 1);
        assert_eq!(grouping.multicast_groups[0].locator, group);
        assert_eq!(grouping.multicast_groups[0].reader_list, vec!["a", "b"]);
    }

    #[test]
    fn targets_of_different_groups_stay_apart() {
        let first = subscription_with_groups(&[group_locator(1)]);
        let second = subscription_with_groups(&[group_locator(2)]);

        let grouping =
            group_targets_by_multicast([("a", &first, false), ("b", &second, false)], |_| true);

        assert_eq!(grouping.multicast_groups.len(), 2);
        assert_eq!(grouping.multicast_groups[0].locator, group_locator(1));
        assert_eq!(grouping.multicast_groups[0].reader_list, vec!["a"]);
        assert_eq!(grouping.multicast_groups[1].locator, group_locator(2));
        assert_eq!(grouping.multicast_groups[1].reader_list, vec!["b"]);
    }

    #[test]
    fn ineligible_target_goes_to_unicast_only() {
        let with_group = subscription_with_groups(&[group_locator(1)]);
        let without_group = subscription_with_groups(&[]);

        let grouping = group_targets_by_multicast(
            [("a", &with_group, false), ("b", &without_group, false), ("c", &with_group, true)],
            |_| true,
        );

        assert_eq!(grouping.multicast_groups.len(), 1);
        assert_eq!(grouping.multicast_groups[0].reader_list, vec!["a"]);
        assert_eq!(grouping.unicast_only_reader_list, vec!["b", "c"]);
    }

    #[test]
    fn target_is_keyed_by_its_first_reachable_group() {
        let unreachable = group_locator(1);
        let reachable = group_locator(2);
        let both = subscription_with_groups(&[unreachable.clone(), reachable.clone()]);
        let only_second = subscription_with_groups(&[reachable.clone()]);

        let grouping = group_targets_by_multicast(
            [("a", &both, false), ("b", &only_second, false)],
            |locator| locator != &unreachable,
        );

        assert_eq!(grouping.multicast_groups.len(), 1);
        assert_eq!(grouping.multicast_groups[0].locator, reachable);
        assert_eq!(grouping.multicast_groups[0].reader_list, vec!["a", "b"]);
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
