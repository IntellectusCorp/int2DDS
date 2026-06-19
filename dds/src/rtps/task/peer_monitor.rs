//! Peer connection monitor.
//!
//! Listens for dead peer events from the transport layer (e.g. TCP keepalive timeout)
//! and triggers RTPS-level cleanup of all state associated with the disconnected peer
//! (proxies, participant proxy data, matched status updates).

use std::net::SocketAddr;
use std::sync::{Arc, Weak};
use std::thread::{self, JoinHandle};

use crossbeam_channel::Receiver;
use log::{debug, warn};

use crate::rtps::common::entity_id::EntityId;
use crate::rtps::common::guid::{Guid, GuidPrefix};
use crate::rtps::entities::participant::Participant;

pub(crate) struct PeerMonitor {
    participant: Weak<Participant>,
    dead_peer_rx: Receiver<SocketAddr>,
    thread_handle: Option<JoinHandle<()>>,
}

impl PeerMonitor {
    pub(crate) fn new(participant: &Arc<Participant>, dead_peer_rx: Receiver<SocketAddr>) -> Self {
        Self { participant: Arc::downgrade(participant), dead_peer_rx, thread_handle: None }
    }

    pub(crate) fn start(&mut self) {
        let participant_weak = self.participant.clone();
        let rx = self.dead_peer_rx.clone();

        let handle = thread::Builder::new()
            .name("peer_monitor".to_string())
            .spawn(move || {
                while let Ok(dead_addr) = rx.recv() {
                    if let Some(participant) = participant_weak.upgrade() {
                        let guid_prefix = Self::resolve_guid_prefix(&participant, &dead_addr);
                        match guid_prefix {
                            Some(prefix) => {
                                warn!(
                                    "[PeerMonitor] Peer {:?} lost (addr={:?}), cleaning up RTPS state",
                                    prefix, dead_addr
                                );
                                let participant_guid = Guid::new(prefix, EntityId::PARTICIPANT);
                                let _ = participant.unmatch_with_remote_participant(&participant_guid);
                            }
                            None => {
                                warn!(
                                    "[PeerMonitor] Dead peer addr={:?} not found in SPDP data, skipping cleanup",
                                    dead_addr
                                );
                            }
                        }
                    } else {
                        break;
                    }
                }
                debug!("[PeerMonitor] Thread finished");
            })
            .expect("Failed to spawn peer_monitor thread");

        self.thread_handle = Some(handle);
    }

    /// Resolve the RTPS GuidPrefix of a dead peer from its SocketAddr by searching
    /// the SPDP discovered participant data for a matching locator (IP + port).
    fn resolve_guid_prefix(
        participant: &Arc<Participant>,
        addr: &SocketAddr,
    ) -> Option<GuidPrefix> {
        let ip = match addr {
            SocketAddr::V4(v4) => *v4.ip(),
            SocketAddr::V6(_) => return None,
        };
        let port = addr.port() as u32;

        let proxy_datas = participant.remote_participant_proxy_datas();
        let guard = proxy_datas.lock().ok()?;

        for proxy in guard.iter() {
            let locators = proxy
                .metatraffic_unicast_locator_list()
                .iter()
                .chain(proxy.default_unicast_locator_list().iter());

            for locator in locators {
                if locator.to_ip_v4_addr() == ip && locator.access_port() == port {
                    return Some(proxy.guid_prefix());
                }
            }
        }
        None
    }
}
