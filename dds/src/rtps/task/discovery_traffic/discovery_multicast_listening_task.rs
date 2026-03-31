use crate::common::instance_handle::InstanceHandle;
use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::common::types::DomainId;
use crate::rtps::entities::entity::Entity;
use crate::rtps::entities::participant::Participant;
use crate::rtps::logic::message_processor::participant_message_processor::ParticipantMessageProcessor as _;
use crate::rtps::logic::spdp_logic::SpdpLogic;
use crate::rtps::messages::message_receiver::MessageReceiver;
use crate::rtps::transport::plugin::MessageSource;
use crate::rtps::transport::socket::MAX_EVENTS;
use crate::serialize::pl_cdr::InlineQosParameters;
use log::{debug, error, info, warn};
use mio::{Events, Interest, Poll, Token};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

pub(crate) struct DiscoveryMulticastListeningTask {
    guid_prefix: GuidPrefix,
    domain_id: DomainId,
    spdp_logic: Arc<Option<SpdpLogic>>,
}

impl DiscoveryMulticastListeningTask {
    pub(crate) fn new(participant: Arc<Participant>) -> Self {
        let guid_prefix = participant.guid().prefix();
        let domain_id = participant.domain_id();
        let (spdp_logic, _, _) = participant.get_logics();
        Self { guid_prefix, domain_id, spdp_logic }
    }

    pub(crate) fn multicast_listening(&mut self, source: MessageSource) -> std::io::Result<()> {
        match source {
            MessageSource::MioPoll { mut listener } => self.listen_mio_poll(&mut listener),
            MessageSource::Channel { rx } => self.listen_channel(&rx),
        }
    }

    fn listen_mio_poll(
        &mut self,
        listener: &mut crate::rtps::transport::udp::udp_listener::UdpListener,
    ) -> std::io::Result<()> {
        info!("start discovery multicast listening (MioPoll)");
        let mut poll = Poll::new()?;
        let mut events = Events::with_capacity(MAX_EVENTS);

        let token = Token(listener.socket().local_addr().unwrap().port() as usize);
        poll.registry().register(listener.socket(), token, Interest::READABLE)?;

        loop {
            match poll.poll(&mut events, Some(Duration::from_millis(100))) {
                Ok(()) => {}
                Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => {
                    warn!(
                        "Poll interrupted in discovery multicast listening task, continuing: {}",
                        e
                    );
                    continue;
                }
                Err(e) => return Err(e),
            }

            let spdp_logic =
                self.spdp_logic.as_ref().as_ref().expect("SpdpLogic is not initialized");
            if let Ok(true) = spdp_logic.is_participant_terminated() {
                debug!("Detected global termination flag, discovery multicast listening loop is terminating...");
                let _ = poll.registry().deregister(listener.socket());
                listener.close();
                return Ok(());
            }

            for event in events.iter() {
                if event.token() == token && event.is_readable() {
                    while let Some((buffer, from_addr)) = listener.get_message() {
                        let spdp_logic = self
                            .spdp_logic
                            .as_ref()
                            .as_ref()
                            .expect("SpdpLogic is not initialized");
                        if let Ok(true) = spdp_logic.is_participant_terminated() {
                            debug!("Detected global termination flag, discovery multicast listening loop is terminating...");
                            let _ = poll.registry().deregister(listener.socket());
                            return Ok(());
                        }

                        self.process_rtps_message(&buffer, from_addr);
                    }
                }
            }
        }
    }

    fn listen_channel(
        &mut self,
        rx: &crossbeam_channel::Receiver<crate::rtps::transport::plugin::IncomingMessage>,
    ) -> std::io::Result<()> {
        info!("start discovery multicast listening (Channel)");

        loop {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(msg) => {
                    let spdp_logic =
                        self.spdp_logic.as_ref().as_ref().expect("SpdpLogic is not initialized");
                    if let Ok(true) = spdp_logic.is_participant_terminated() {
                        debug!("Detected global termination flag, discovery multicast channel listening terminating...");
                        return Ok(());
                    }
                    self.process_rtps_message(&msg.data, msg.source);
                }
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                    let spdp_logic =
                        self.spdp_logic.as_ref().as_ref().expect("SpdpLogic is not initialized");
                    if let Ok(true) = spdp_logic.is_participant_terminated() {
                        debug!("Detected global termination flag, discovery multicast channel listening terminating...");
                        return Ok(());
                    }
                }
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                    info!("[DiscoveryMulticast] Channel disconnected, stopping listener");
                    return Ok(());
                }
            }
        }
    }

    fn process_rtps_message(&mut self, buffer: &[u8], from_addr: SocketAddr) {
        let mut message_receiver = MessageReceiver::new(self.guid_prefix, &from_addr);
        let rtps_message = message_receiver.init(&bytes::Bytes::copy_from_slice(buffer));
        if rtps_message.is_err() {
            error!("Failed to parse RTPS message from {:?}", from_addr);
            return;
        }

        // Ignore messages sent by myself
        let rtps_message = rtps_message.unwrap();
        if rtps_message.header.guid_prefix() == self.guid_prefix {
            debug!(
                "[multicast] Filtering out self-sent message - guid_prefix: {:?}",
                self.guid_prefix
            );
            return;
        }

        let participant_proxy_data =
            message_receiver.extract_participant_proxy_data(self.domain_id);

        match participant_proxy_data {
            Some((participant_proxy_data, inline_qos_params)) => {
                let spdp_logic =
                    self.spdp_logic.as_ref().as_ref().expect("SpdpLogic is not initialized");

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
                    error!("Failed to handle discovered participant data: {:?}", e);
                }
            }
            None => {
                error!("Failed to parse RTPS message from {:?}", from_addr);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::rtps::entities::participant::Participant;
    use crate::rtps::transport::socket::Socket;

    #[test]
    #[ignore]
    fn test_discovery_multicast_receive() {
        let domain_id = 10;
        let _socket = Socket::new(domain_id);
        let _participant = Arc::new(Participant::new(
            domain_id,
            _socket.participant_id(),
            _socket.working_ips(),
            None,
        ));

        // TODO: create transport and get MessageSource for discovery multicast
        // let mut task = DiscoveryMulticastListeningTask::new(participant);
        // let _ = task.multicast_listening(source);
    }
}
