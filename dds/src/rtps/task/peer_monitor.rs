//! Peer connection monitor.
//!
//! Listens for dead peer events from the transport layer (e.g. TCP keepalive timeout)
//! and triggers RTPS-level cleanup of all state associated with the disconnected peer
//! (proxies, participant proxy data, matched status updates).

use std::sync::{Arc, Weak};
use std::thread::{self, JoinHandle};

use crossbeam_channel::Receiver;
use log::{debug, warn};

use crate::rtps::common::entity_id::EntityId;
use crate::rtps::common::guid::{Guid, GuidPrefix};
use crate::rtps::entities::participant::Participant;

pub(crate) struct PeerMonitor {
    participant: Weak<Participant>,
    dead_peer_rx: Receiver<GuidPrefix>,
    thread_handle: Option<JoinHandle<()>>,
}

impl PeerMonitor {
    pub(crate) fn new(
        participant: &Arc<Participant>,
        dead_peer_rx: Receiver<GuidPrefix>,
    ) -> Self {
        Self {
            participant: Arc::downgrade(participant),
            dead_peer_rx,
            thread_handle: None,
        }
    }

    pub(crate) fn start(&mut self) {
        let participant_weak = self.participant.clone();
        let rx = self.dead_peer_rx.clone();

        let handle = thread::Builder::new()
            .name("peer_monitor".to_string())
            .spawn(move || {
                while let Ok(guid_prefix) = rx.recv() {
                    if let Some(participant) = participant_weak.upgrade() {
                        warn!(
                            "[PeerMonitor] Peer {:?} lost, cleaning up RTPS state",
                            guid_prefix
                        );
                        let participant_guid = Guid::new(guid_prefix, EntityId::PARTICIPANT);
                        participant.unmatch_with_remote_participant(&participant_guid);
                    } else {
                        break;
                    }
                }
                debug!("[PeerMonitor] Thread finished");
            })
            .expect("Failed to spawn peer_monitor thread");

        self.thread_handle = Some(handle);
    }
}
