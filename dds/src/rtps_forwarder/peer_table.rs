//! Where a participant lives, keyed by GUID prefix.
//!
//! Every forwarded datagram is routed by this table alone: a prefix is either a
//! participant on the forwarder's own network, in which case its real addresses
//! are known, or a participant behind one of the peer forwarders, in which case
//! the only way to reach it is the link it was learned on. An entry lives as
//! long as the lease its owner announced, so a participant that stops
//! announcing is forgotten even when it never got to say goodbye.

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use super::{discovery_rewrite::SpdpEndpoints, link::LinkId, rtps_scan::GuidPrefix};

/// Lease given to an announcement that does not state one.
pub(crate) const DEFAULT_LEASE: Duration = Duration::from_secs(100);

/// A shorter lease would expire between two announcements, and a longer one is
/// indistinguishable from never expiring.
const MIN_LEASE: Duration = Duration::from_secs(1);
const MAX_LEASE: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PeerRoute {
    Local(SpdpEndpoints),
    Remote(LinkId),
}

struct Entry {
    route: PeerRoute,
    expires_at: Instant,
}

#[derive(Default)]
pub(crate) struct PeerTable {
    routes: Mutex<HashMap<GuidPrefix, Entry>>,
}

impl PeerTable {
    pub(crate) fn insert_local(
        &self,
        prefix: GuidPrefix,
        endpoints: SpdpEndpoints,
        lease: Duration,
        now: Instant,
    ) {
        self.insert(prefix, PeerRoute::Local(endpoints), lease, now);
    }

    pub(crate) fn insert_remote(
        &self,
        prefix: GuidPrefix,
        link: LinkId,
        lease: Duration,
        now: Instant,
    ) {
        self.insert(prefix, PeerRoute::Remote(link), lease, now);
    }

    pub(crate) fn remove(&self, prefix: &GuidPrefix) {
        self.lock().remove(prefix);
    }

    /// Drops every participant reached through one link, leaving the other
    /// links alone. Returns how many were dropped so the caller can say so.
    pub(crate) fn forget_remote(&self, link: LinkId) -> usize {
        let mut routes = self.lock();
        let before = routes.len();
        routes.retain(|_, entry| entry.route != PeerRoute::Remote(link));
        before - routes.len()
    }

    pub(crate) fn purge_expired(&self, now: Instant) -> usize {
        let mut routes = self.lock();
        let before = routes.len();
        routes.retain(|_, entry| entry.expires_at > now);
        before - routes.len()
    }

    pub(crate) fn route(&self, prefix: &GuidPrefix) -> Option<PeerRoute> {
        self.lock().get(prefix).map(|entry| entry.route)
    }

    /// How many of this network's participants are currently carried, which is
    /// what the configured cap is weighed against.
    pub(crate) fn local_count(&self) -> usize {
        self.lock().values().filter(|entry| matches!(entry.route, PeerRoute::Local(_))).count()
    }

    pub(crate) fn is_local(&self, prefix: &GuidPrefix) -> bool {
        matches!(self.route(prefix), Some(PeerRoute::Local(_)))
    }

    pub(crate) fn is_remote(&self, prefix: &GuidPrefix) -> bool {
        matches!(self.route(prefix), Some(PeerRoute::Remote(_)))
    }

    /// Which link reaches this participant, if any does.
    pub(crate) fn remote_link(&self, prefix: &GuidPrefix) -> Option<LinkId> {
        match self.route(prefix) {
            Some(PeerRoute::Remote(link)) => Some(link),
            _ => None,
        }
    }

    fn insert(&self, prefix: GuidPrefix, route: PeerRoute, lease: Duration, now: Instant) {
        let lease = lease.clamp(MIN_LEASE, MAX_LEASE);
        let expires_at = now.checked_add(lease).unwrap_or(now);
        self.lock().insert(prefix, Entry { route, expires_at });
    }

    /// A poisoned table would strand every route, so the lock is recovered
    /// instead of propagating the panic into the forwarder threads.
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<GuidPrefix, Entry>> {
        self.routes.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, SocketAddrV4};

    use super::*;

    const PREFIX: GuidPrefix = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
    const OTHER_PREFIX: GuidPrefix = [12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1];
    const LEASE: Duration = Duration::from_secs(10);
    const LINK: LinkId = LinkId(0);
    const OTHER_LINK: LinkId = LinkId(1);

    fn endpoints() -> SpdpEndpoints {
        SpdpEndpoints {
            metatraffic: SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 1), 7410),
            user_data: SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 1), 7411),
        }
    }

    #[test]
    fn unknown_prefix_has_no_route() {
        let table = PeerTable::default();
        assert_eq!(table.route(&PREFIX), None);
        assert!(!table.is_local(&PREFIX));
        assert!(!table.is_remote(&PREFIX));
    }

    #[test]
    fn local_and_remote_routes_are_distinguished() {
        let table = PeerTable::default();
        let now = Instant::now();

        table.insert_local(PREFIX, endpoints(), LEASE, now);
        assert_eq!(table.route(&PREFIX), Some(PeerRoute::Local(endpoints())));
        assert!(table.is_local(&PREFIX));

        table.insert_remote(PREFIX, LINK, LEASE, now);
        assert!(table.is_remote(&PREFIX));
        assert!(!table.is_local(&PREFIX));
    }

    #[test]
    fn only_this_networks_participants_are_counted() {
        let table = PeerTable::default();
        let now = Instant::now();
        table.insert_local(PREFIX, endpoints(), LEASE, now);
        table.insert_remote(OTHER_PREFIX, LINK, LEASE, now);

        assert_eq!(table.local_count(), 1);
    }

    #[test]
    fn a_departed_participant_is_removed() {
        let table = PeerTable::default();
        table.insert_local(PREFIX, endpoints(), LEASE, Instant::now());

        table.remove(&PREFIX);
        assert_eq!(table.route(&PREFIX), None);
    }

    #[test]
    fn an_entry_lasts_exactly_its_lease() {
        let table = PeerTable::default();
        let now = Instant::now();
        table.insert_local(PREFIX, endpoints(), LEASE, now);

        assert_eq!(table.purge_expired(now + LEASE - Duration::from_millis(1)), 0);
        assert!(table.is_local(&PREFIX));

        assert_eq!(table.purge_expired(now + LEASE), 1);
        assert_eq!(table.route(&PREFIX), None);
    }

    #[test]
    fn a_repeated_announcement_extends_the_lease() {
        let table = PeerTable::default();
        let now = Instant::now();
        table.insert_local(PREFIX, endpoints(), LEASE, now);

        let later = now + LEASE / 2;
        table.insert_local(PREFIX, endpoints(), LEASE, later);

        assert_eq!(table.purge_expired(now + LEASE + Duration::from_millis(1)), 0);
        assert!(table.is_local(&PREFIX));
    }

    #[test]
    fn an_absurd_lease_is_clamped_instead_of_overflowing() {
        let table = PeerTable::default();
        let now = Instant::now();
        table.insert_remote(PREFIX, LINK, Duration::MAX, now);
        table.insert_remote(OTHER_PREFIX, LINK, Duration::ZERO, now);

        assert_eq!(table.purge_expired(now + Duration::from_secs(2)), 1);
        assert!(table.is_remote(&PREFIX));
        assert_eq!(table.route(&OTHER_PREFIX), None);
    }

    #[test]
    fn losing_one_link_leaves_the_other_links_alone() {
        let table = PeerTable::default();
        let now = Instant::now();
        table.insert_remote(PREFIX, LINK, LEASE, now);
        table.insert_remote(OTHER_PREFIX, OTHER_LINK, LEASE, now);

        assert_eq!(table.forget_remote(LINK), 1);
        assert_eq!(table.route(&PREFIX), None);
        assert_eq!(table.remote_link(&OTHER_PREFIX), Some(OTHER_LINK));
    }

    #[test]
    fn losing_the_link_forgets_only_the_far_network() {
        let table = PeerTable::default();
        let now = Instant::now();
        table.insert_local(PREFIX, endpoints(), LEASE, now);
        table.insert_remote(OTHER_PREFIX, LINK, LEASE, now);

        assert_eq!(table.forget_remote(LINK), 1);
        assert!(table.is_local(&PREFIX));
        assert_eq!(table.route(&OTHER_PREFIX), None);
    }
}
