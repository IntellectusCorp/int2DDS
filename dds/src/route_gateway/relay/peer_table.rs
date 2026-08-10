//! Where a participant lives, keyed by GUID prefix.
//!
//! Every relayed datagram is routed by this table alone: a prefix is either a
//! participant on the gateway's own network, in which case its real addresses
//! are known, or a participant behind the peer gateway, in which case the only
//! way to reach it is the link.

use std::{collections::HashMap, sync::Mutex};

use super::{rtps_scan::GuidPrefix, spdp_rewrite::SpdpEndpoints};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PeerRoute {
    Local(SpdpEndpoints),
    Remote,
}

#[derive(Default)]
pub(crate) struct PeerTable {
    routes: Mutex<HashMap<GuidPrefix, PeerRoute>>,
}

impl PeerTable {
    pub(crate) fn insert_local(&self, prefix: GuidPrefix, endpoints: SpdpEndpoints) {
        self.lock().insert(prefix, PeerRoute::Local(endpoints));
    }

    pub(crate) fn insert_remote(&self, prefix: GuidPrefix) {
        self.lock().insert(prefix, PeerRoute::Remote);
    }

    pub(crate) fn route(&self, prefix: &GuidPrefix) -> Option<PeerRoute> {
        self.lock().get(prefix).copied()
    }

    pub(crate) fn is_local(&self, prefix: &GuidPrefix) -> bool {
        matches!(self.route(prefix), Some(PeerRoute::Local(_)))
    }

    pub(crate) fn is_remote(&self, prefix: &GuidPrefix) -> bool {
        matches!(self.route(prefix), Some(PeerRoute::Remote))
    }

    /// A poisoned table would strand every route, so the lock is recovered
    /// instead of propagating the panic into the relay threads.
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<GuidPrefix, PeerRoute>> {
        self.routes.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, SocketAddrV4};

    use super::*;

    const PREFIX: GuidPrefix = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];

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
        table.insert_local(PREFIX, endpoints());
        assert_eq!(table.route(&PREFIX), Some(PeerRoute::Local(endpoints())));
        assert!(table.is_local(&PREFIX));

        table.insert_remote(PREFIX);
        assert!(table.is_remote(&PREFIX));
        assert!(!table.is_local(&PREFIX));
    }
}
