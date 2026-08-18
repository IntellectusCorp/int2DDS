use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;

use bytes::Bytes;
use flume::{Receiver, RecvTimeoutError, Sender, TrySendError};
use log::{debug, error, info};

use crate::common::instance_handle::InstanceHandle;
use crate::rtps::common::entity_id::EntityId;
use crate::rtps::common::entity_kind::EntityKind;
use crate::rtps::common::guid::{Guid, GuidPrefix};
use crate::rtps::common::rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult};
use crate::rtps::common::types::DomainId;
use crate::rtps::entities::participant::Participant;
use crate::rtps::logic::message_processor::participant_message_processor::ParticipantMessageProcessor as _;
use crate::rtps::logic::message_processor::unicast_message_processor::UnicastMessageProcessor as _;
use crate::rtps::logic::sedp_logic::SedpLogic;
use crate::rtps::logic::spdp_logic::SpdpLogic;
use crate::rtps::messages::message_receiver::{MessageReceiver, TypedSubmessage};
use crate::serialize::pl_cdr::InlineQosParameters;

// Discovery traffic is far lighter than user data, but losing an announcement
// recovers slowly, so the byte cap stays generous. The count backstop guards
// the channel itself.
pub(crate) const DISCOVERY_RECV_QUEUE_BYTE_CAP: usize = 32 * 1024 * 1024;
pub(crate) const DISCOVERY_RECV_QUEUE_COUNT_BACKSTOP: usize = 32_768;

// Which socket a queued discovery datagram arrived on, so the worker runs the
// matching processing.
#[derive(Clone, Copy)]
pub(crate) enum DiscoverySource {
    Unicast,
    Multicast,
}

// Hand a datagram to the discovery worker without blocking the socket thread.
// The byte cap bounds queued payload; over it we drop and rely on retransmit.
pub(crate) fn enqueue_discovery(
    tx: &Sender<(Bytes, SocketAddr, DiscoverySource)>,
    queued_bytes: &AtomicUsize,
    buffer: Bytes,
    from_addr: SocketAddr,
    source: DiscoverySource,
) {
    let len = buffer.len();

    if queued_bytes.load(Ordering::Acquire) + len > DISCOVERY_RECV_QUEUE_BYTE_CAP {
        debug!(
            "discovery recv queue over byte cap, dropping datagram from {:?} (len {})",
            from_addr, len
        );
        return;
    }

    match tx.try_send((buffer, from_addr, source)) {
        Ok(()) => {
            queued_bytes.fetch_add(len, Ordering::AcqRel);
        }
        Err(TrySendError::Full(_)) => {
            debug!(
                "discovery recv queue count backstop full, dropping datagram from {:?}",
                from_addr
            );
        }
        Err(TrySendError::Disconnected(_)) => {
            debug!("discovery worker gone, dropping datagram from {:?}", from_addr);
        }
    }
}

// Drains the discovery channel off the socket threads and runs the SPDP/SEDP
// processing the discovery listening threads used to run inline. Both the
// unicast and multicast sockets feed this single worker.
pub(crate) struct DiscoveryWorker {
    guid_prefix: GuidPrefix,
    domain_id: DomainId,
    participant: Weak<Participant>,
    spdp_logic: Arc<Option<SpdpLogic>>,
    sedp_logic: Arc<Option<SedpLogic>>,
    rx: Receiver<(Bytes, SocketAddr, DiscoverySource)>,
    queued_bytes: Arc<AtomicUsize>,
}

impl DiscoveryWorker {
    pub(crate) fn new(
        guid_prefix: GuidPrefix,
        domain_id: DomainId,
        participant: Weak<Participant>,
        spdp_logic: Arc<Option<SpdpLogic>>,
        sedp_logic: Arc<Option<SedpLogic>>,
        rx: Receiver<(Bytes, SocketAddr, DiscoverySource)>,
        queued_bytes: Arc<AtomicUsize>,
    ) -> Self {
        Self { guid_prefix, domain_id, participant, spdp_logic, sedp_logic, rx, queued_bytes }
    }

    pub(crate) fn run(&self) {
        let participant = match self.participant.upgrade() {
            Some(participant) => participant,
            None => {
                debug!("discovery worker: participant already dropped before start");
                return;
            }
        };

        loop {
            match self.rx.recv_timeout(Duration::from_millis(100)) {
                Ok((bytes, from_addr, source)) => {
                    self.queued_bytes.fetch_sub(bytes.len(), Ordering::AcqRel);

                    if participant.is_terminated() {
                        debug!("discovery worker detected termination, stopping");
                        return;
                    }

                    let result = match source {
                        DiscoverySource::Unicast => self.process_unicast(bytes, from_addr),
                        DiscoverySource::Multicast => self.process_multicast(bytes, from_addr),
                    };
                    if let Err(e) = result {
                        debug!(
                            "discovery worker failed to process message from {:?}: {:?}",
                            from_addr, e
                        );
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    if participant.is_terminated() {
                        debug!("discovery worker detected termination while idle, stopping");
                        return;
                    }
                }
                Err(RecvTimeoutError::Disconnected) => {
                    info!("discovery worker channel disconnected, stopping");
                    return;
                }
            }
        }
    }

    fn process_unicast(&self, bytes: Bytes, from_addr: SocketAddr) -> RtpsResult<()> {
        let mut message_receiver = MessageReceiver::new(self.guid_prefix, &from_addr);
        let rtps_message = message_receiver.init(&bytes)?;

        // Ignore messages sent by myself
        if rtps_message.header.guid_prefix() == self.guid_prefix {
            debug!(
                "[DiscoveryUnicast] Filtering out self-sent message - guid_prefix: {}",
                Guid::guid_prefix_to_string(&self.guid_prefix)
            );
            return Ok(());
        }

        // TCP mode sends SPDP via unicast
        if self.is_spdp_message(&message_receiver) {
            let participant_proxy_data =
                message_receiver.extract_participant_proxy_data(self.domain_id);
            if let Some((participant_proxy_data, inline_qos_params)) = participant_proxy_data {
                let spdp_logic = self.spdp_logic.as_ref().as_ref().ok_or_else(|| {
                    RtpsError::new(RtpsErrorCode::DataNotSet, "SpdpLogic is not initialized")
                })?;

                let is_termination = inline_qos_params
                    .as_ref()
                    .and_then(|qos| qos.get_status_info())
                    .is_some_and(|status| status.disposed() || status.unregistered());

                if is_termination {
                    let terminated_participant_guid = inline_qos_params
                        .as_ref()
                        .and_then(|qos| qos.get_key_hash())
                        .unwrap_or_else(|| {
                            InstanceHandle::from_guid(&participant_proxy_data.participant_guid())
                        });

                    if let Err(e) = spdp_logic.handle_participant_termination_message(
                        &terminated_participant_guid.to_guid(),
                    ) {
                        error!("Failed to handle participant termination message: {:?}", e);
                    }
                } else if let Err(e) =
                    spdp_logic.handle_discovered_participant_data(participant_proxy_data)
                {
                    error!("[DiscoveryUnicast] Failed to handle SPDP data: {:?}", e);
                }
            }
            return Ok(());
        }

        // SEDP
        let participant = self.participant.upgrade().ok_or_else(|| {
            RtpsError::new(RtpsErrorCode::ArcUpgradeError, "Participant already dropped")
        })?;
        let mut sedp_logic = self
            .sedp_logic
            .as_ref()
            .as_ref()
            .ok_or_else(|| {
                RtpsError::new(RtpsErrorCode::DataNotSet, "SedpLogic is not initialized")
            })?
            .clone();

        sedp_logic.handle_rtps_message(message_receiver.clone())?;
        if let Some(mut wlp_logic) = participant.wlp_logic() {
            wlp_logic.handle_rtps_message(message_receiver)?;
        }
        Ok(())
    }

    fn process_multicast(&self, bytes: Bytes, from_addr: SocketAddr) -> RtpsResult<()> {
        let mut message_receiver = MessageReceiver::new(self.guid_prefix, &from_addr);
        let rtps_message = message_receiver.init(&bytes)?;

        // Ignore messages sent by myself
        if rtps_message.header.guid_prefix() == self.guid_prefix {
            debug!(
                "[multicast] Filtering out self-sent message - guid_prefix: {}",
                Guid::guid_prefix_to_string(&self.guid_prefix)
            );
            return Ok(());
        }

        let (participant_proxy_data, inline_qos_params) =
            match message_receiver.extract_participant_proxy_data(self.domain_id) {
                Some(data) => data,
                None => {
                    debug!("[multicast] No participant proxy data from {:?}", from_addr);
                    return Ok(());
                }
            };

        if participant_proxy_data.participant_guid().entity_id().entity_kind()
            != EntityKind::BUILT_IN_PARTICIPANT
        {
            debug!(
                "Received RTPS message with non-participant entity ID: {}. Ignoring.",
                participant_proxy_data.participant_guid()
            );
            return Ok(());
        }

        let spdp_logic = self.spdp_logic.as_ref().as_ref().ok_or_else(|| {
            RtpsError::new(RtpsErrorCode::DataNotSet, "SpdpLogic is not initialized")
        })?;

        let is_termination = inline_qos_params
            .as_ref()
            .and_then(|qos| qos.get_status_info())
            .is_some_and(|status| status.disposed() || status.unregistered());

        if is_termination {
            let terminated_participant_guid =
                inline_qos_params.as_ref().and_then(|qos| qos.get_key_hash()).unwrap_or_else(
                    || InstanceHandle::from_guid(&participant_proxy_data.participant_guid()),
                );

            if let Err(e) = spdp_logic
                .handle_participant_termination_message(&terminated_participant_guid.to_guid())
            {
                error!("Failed to handle participant termination message: {:?}", e);
            }
        } else if let Err(e) = spdp_logic.handle_discovered_participant_data(participant_proxy_data)
        {
            error!("Failed to handle discovered participant data: {:?}", e);
        }
        Ok(())
    }

    fn is_spdp_message(&self, message_receiver: &MessageReceiver) -> bool {
        for submessage in message_receiver.parse_submessages() {
            if let TypedSubmessage::Data(_, data) = submessage {
                if data.writer_id == EntityId::SPDP_BUILTIN_PARTICIPANT_WRITER {
                    return true;
                }
            }
        }
        false
    }
}
