use bytes::Bytes;

use crate::common::instance_handle::InstanceHandle;
use crate::rtps::common::guid::{Guid, GuidPrefix};
use crate::rtps::common::rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult};
use crate::rtps::common::types::DomainId;
use crate::rtps::entities::entity::Entity;
use crate::rtps::entities::participant::Participant;
use crate::rtps::logic::message_processor::participant_message_processor::ParticipantMessageProcessor as _;
use crate::rtps::logic::message_processor::unicast_message_processor::UnicastMessageProcessor as _;
use crate::rtps::logic::sedp_logic::SedpLogic;
use crate::rtps::logic::spdp_logic::SpdpLogic;
use crate::rtps::messages::message_receiver::MessageReceiver;
use crate::rtps::transport::plugin::MessageSource;
use crate::rtps::transport::socket::MAX_EVENTS;
use crate::rtps::transport::tokens::ListenerToken;
use crate::serialize::pl_cdr::InlineQosParameters;
use log::{debug, error, info, warn};
use mio::{Events, Interest, Poll, Waker};
use std::net::SocketAddr;
use std::sync::{Arc, OnceLock, Weak};
use std::time::Duration;

pub(crate) struct DiscoveryUnicastListeningTask {
    guid_prefix: GuidPrefix,
    domain_id: DomainId,
    participant: Weak<Participant>,
    spdp_logic: Arc<Option<SpdpLogic>>,
    sedp_logic: Arc<Option<SedpLogic>>,
    shutdown_waker: Arc<OnceLock<Arc<Waker>>>,
}

impl DiscoveryUnicastListeningTask {
    pub(crate) fn new(participant: Arc<Participant>) -> Self {
        let (spdp_logic, sedp_logic, _) = participant.get_logics();
        let guid_prefix = participant.guid().prefix();
        let domain_id = participant.domain_id();
        Self {
            guid_prefix,
            domain_id,
            participant: Arc::downgrade(&participant),
            spdp_logic,
            sedp_logic,
            shutdown_waker: Arc::new(OnceLock::new()),
        }
    }

    pub(crate) fn set_shutdown_waker(&mut self, handle: Arc<OnceLock<Arc<Waker>>>) {
        self.shutdown_waker = handle;
    }

    pub(crate) fn unicast_listening(&mut self, source: MessageSource) -> std::io::Result<()> {
        match source {
            MessageSource::MioPoll { mut listener } => self.listen_mio_poll(&mut listener),
            MessageSource::Channel { rx } => self.listen_channel(&rx),
            MessageSource::MioPollWithShm { .. } => {
                unreachable!("MioPollWithShm is only used by user-data unicast")
            }
            MessageSource::Stream { .. } => {
                unreachable!("Stream is handled by the stream unicast listening task")
            }
        }
    }

    fn listen_mio_poll(
        &mut self,
        listener: &mut crate::rtps::transport::udp::udp_listener::UdpListener,
    ) -> std::io::Result<()> {
        info!("start discovery unicast listening (MioPoll)");
        let mut poll = Poll::new()?;
        let mut events = Events::with_capacity(MAX_EVENTS);

        let waker = Arc::new(Waker::new(poll.registry(), ListenerToken::Shutdown.to_mio())?);
        let _ = self.shutdown_waker.set(waker);

        let port = listener.socket().local_addr().unwrap().port();
        let token = ListenerToken::Udp(port).to_mio();
        poll.registry().register(listener.socket(), token, Interest::READABLE)?;
        info!("[DiscoveryUnicast] UDP listener registered on port {}", port);

        let participant = self
            .participant
            .upgrade()
            .ok_or_else(|| std::io::Error::other("Participant already dropped"))?;

        loop {
            match poll.poll(&mut events, Some(Duration::from_millis(100))) {
                Ok(()) => {}
                Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => {
                    warn!(
                        "Poll interrupted in discovery unicast listening task, continuing: {}",
                        e
                    );
                    continue;
                }
                Err(e) => return Err(e),
            }

            if participant.is_terminated() {
                debug!("Detected global termination flag, discovery unicast listening loop is terminating...");
                let _ = poll.registry().deregister(listener.socket());
                return Ok(());
            }

            for event in events.iter() {
                if event.token() == token && event.is_readable() {
                    while let Some((buffer, from_addr)) = listener.get_message() {
                        if participant.is_terminated() {
                            debug!("Detected global termination flag during UDP processing");
                            let _ = poll.registry().deregister(listener.socket());
                            return Ok(());
                        }
                        let _ = self.process_rtps_message(buffer, from_addr);
                    }
                }
            }
        }
    }

    fn listen_channel(
        &mut self,
        rx: &flume::Receiver<crate::rtps::transport::plugin::IncomingMessage>,
    ) -> std::io::Result<()> {
        info!("start discovery unicast listening (Channel)");

        let participant = self
            .participant
            .upgrade()
            .ok_or_else(|| std::io::Error::other("Participant already dropped"))?;

        loop {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(msg) => {
                    if participant.is_terminated() {
                        debug!("Detected global termination flag, discovery unicast channel listening terminating...");
                        return Ok(());
                    }
                    let _ = self.process_rtps_message(msg.data, msg.source);
                }
                Err(flume::RecvTimeoutError::Timeout) => {
                    if participant.is_terminated() {
                        debug!("Detected global termination flag, discovery unicast channel listening terminating...");
                        return Ok(());
                    }
                }
                Err(flume::RecvTimeoutError::Disconnected) => {
                    info!("[DiscoveryUnicast] Channel disconnected, stopping listener");
                    return Ok(());
                }
            }
        }
    }

    pub(crate) fn process_rtps_message(
        &mut self,
        bytes: Bytes,
        from_addr: SocketAddr,
    ) -> RtpsResult<()> {
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

        // Check if this is an SPDP message (TCP mode sends SPDP via unicast)
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

        // SEDP processing
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

    /// Check if the RTPS message contains SPDP data (writer_id == SPDP_BUILTIN_PARTICIPANT_WRITER)
    fn is_spdp_message(&self, message_receiver: &MessageReceiver) -> bool {
        use crate::rtps::common::entity_id::EntityId;
        use crate::rtps::messages::message_receiver::TypedSubmessage;

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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::rtps::common::types::DomainId;
    use crate::rtps::entities::participant::Participant;
    use crate::rtps::transport::socket::Socket;

    #[test]
    #[ignore]
    fn test_discovery_unicast_receive() {
        let domain_id: DomainId = 17;
        let _socket = Socket::new(domain_id);
        let _participant = Arc::new(Participant::new(
            domain_id,
            _socket.participant_id(),
            _socket.working_ips(),
            Vec::new(),
            Vec::new(),
        ));

        // TODO: create transport and take_discovery_source() to get MessageSource
        // let source = transport.take_discovery_source();
        // let mut task = DiscoveryUnicastListeningTask::new(participant);
        // let _ = task.unicast_listening(source);
    }
}
