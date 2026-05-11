use crate::common::instance_handle::InstanceHandle;
use crate::rtps::common::entity_kind::EntityKind;
use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::common::types::DomainId;
use crate::rtps::entities::entity::Entity;
use crate::rtps::entities::participant::Participant;
use crate::rtps::logic::message_processor::participant_message_processor::ParticipantMessageProcessor as _;
use crate::rtps::logic::spdp_logic::SpdpLogic;
use crate::rtps::messages::message_receiver::MessageReceiver;
use crate::rtps::transport::socket::MAX_EVENTS;
use crate::rtps::transport::tokens::ListenerToken;
use crate::rtps::transport::udp::udp_listener::UdpListener;
use crate::serialize::pl_cdr::InlineQosParameters;
use log::{debug, error, info, warn};
use mio::{Events, Interest, Poll, Waker};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

pub(crate) struct DiscoveryMulticastListeningTask {
    guid_prefix: GuidPrefix,
    domain_id: DomainId,
    discovery_multicast_listener: Option<UdpListener>,
    spdp_logic: Arc<Option<SpdpLogic>>,
    shutdown_waker: Arc<OnceLock<Arc<Waker>>>,
}

impl DiscoveryMulticastListeningTask {
    pub(crate) fn new(
        discovery_multicast_listener: Option<UdpListener>,
        participant: Arc<Participant>,
    ) -> Self {
        let guid_prefix = { participant.guid().prefix() };
        let domain_id = { participant.domain_id() };
        let (spdp_logic, _, _) = participant.get_logics();
        Self {
            guid_prefix,
            domain_id,
            discovery_multicast_listener,
            spdp_logic,
            shutdown_waker: Arc::new(OnceLock::new()),
        }
    }

    pub(crate) fn set_shutdown_waker(&mut self, handle: Arc<OnceLock<Arc<Waker>>>) {
        self.shutdown_waker = handle;
    }

    pub(crate) fn multicast_listening(&mut self) -> std::io::Result<()> {
        info!("start discovery multicast listening");
        let mut poll = Poll::new().unwrap();
        let mut events = Events::with_capacity(MAX_EVENTS);

        let waker = Arc::new(Waker::new(poll.registry(), ListenerToken::Shutdown.to_mio())?);
        let _ = self.shutdown_waker.set(waker);

        let listener: &mut UdpListener = match &mut self.discovery_multicast_listener {
            Some(listener) => listener,
            None => {
                return Err(std::io::Error::other("discovery multicast listener task is not set"));
            }
        };
        let port = listener.socket().local_addr().unwrap().port();
        let discovery_multicast_token = ListenerToken::Udp(port).to_mio();
        poll.registry().register(
            listener.socket(),
            discovery_multicast_token,
            Interest::READABLE,
        )?;

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

            // Check for participant termination before processing events
            let spdp_logic =
                self.spdp_logic.as_ref().as_ref().expect("SpdpLogic is not initialized");
            if let Ok(true) = spdp_logic.is_participant_terminated() {
                debug!("Detected global termination flag, discovery multicast listening loop is terminating...");
                // deregister before close
                let _ = poll.registry().deregister(listener.socket());
                listener.close();
                return Ok(());
            }

            for event in events.iter() {
                if event.token() == discovery_multicast_token && event.is_readable() {
                    while let Some((buffer, from_addr)) = listener.get_message() {
                        // Check for participant termination during message processing
                        let spdp_logic = self
                            .spdp_logic
                            .as_ref()
                            .as_ref()
                            .expect("SpdpLogic is not initialized");
                        if let Ok(true) = spdp_logic.is_participant_terminated() {
                            debug!("Detected global termination flag, discovery multicast listening loop is terminating...");
                            // deregister before return
                            let _ = poll.registry().deregister(listener.socket());
                            return Ok(());
                        }

                        let mut message_receiver: MessageReceiver =
                            MessageReceiver::new(self.guid_prefix, &from_addr);
                        let rtps_message = message_receiver.init(&buffer);
                        if rtps_message.is_err() {
                            error!("Failed to parse RTPS message from {:?}", from_addr);
                            continue;
                        }

                        // Ignore messages sent by myself
                        let rtps_message = rtps_message.unwrap();
                        if rtps_message.header.guid_prefix() == self.guid_prefix {
                            debug!(
                                "[multicast] Filtering out self-sent message - guid_prefix: {:?}",
                                self.guid_prefix
                            );
                            continue;
                        }

                        let participant_proxy_data =
                            message_receiver.extract_participant_proxy_data(self.domain_id);

                        match participant_proxy_data {
                            Some((participant_proxy_data, inline_qos_params)) => {
                                if participant_proxy_data
                                    .participant_guid()
                                    .entity_id()
                                    .entity_kind()
                                    != EntityKind::BUILT_IN_PARTICIPANT
                                {
                                    debug!(
                                        "Received RTPS message with non-participant entity ID: {:?}. Ignoring.",
                                        participant_proxy_data.participant_guid()
                                    );
                                    continue;
                                }

                                let spdp_logic = self
                                    .spdp_logic
                                    .as_ref()
                                    .as_ref()
                                    .expect("SpdpLogic is not initialized");

                                let is_termination = inline_qos_params
                                    .as_ref()
                                    .and_then(|qos| qos.get_status_info())
                                    .is_some_and(|status| {
                                        status.disposed() || status.unregistered()
                                    });

                                if is_termination {
                                    let terminated_participant_guid = inline_qos_params
                                        .as_ref()
                                        .and_then(|qos| qos.get_key_hash())
                                        .unwrap_or_else(|| {
                                            InstanceHandle::from_guid(
                                                &participant_proxy_data.participant_guid(),
                                            )
                                        });

                                    if let Err(e) = spdp_logic
                                        .handle_participant_termination_message(
                                            &terminated_participant_guid.to_guid(),
                                        )
                                    {
                                        error!("Failed to handle participant termination message: {:?}", e);
                                    }
                                } else if let Err(e) = spdp_logic
                                    .handle_discovered_participant_data(participant_proxy_data)
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
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::rtps::entities::participant::Participant;
    use crate::rtps::task::discovery_traffic::discovery_multicast_listening_task::DiscoveryMulticastListeningTask;
    use crate::rtps::transport::socket::Socket;
    use crate::rtps::transport::TransportConfig;

    #[test]
    #[ignore]
    fn test_discovery_multicast_receive() {
        let domain_id = 10;
        let mut socket = Socket::new(domain_id, TransportConfig::default()); //domain_id 0
        socket.create_socket();
        let participant =
            Arc::new(Participant::new(domain_id, socket.participant_id(), socket.working_ips()));

        // When socket reset is needed
        // socket.close();
        // return;

        //discovery multicast port : 7400
        //discovery unicast port :  7410
        //user traffic multicast port : 7401
        //user traffic unicast port : 7411
        let mut discovery_multicast_listening_task = DiscoveryMulticastListeningTask::new(
            socket.discovery_multicast_listener(),
            participant,
        );
        let _ = discovery_multicast_listening_task.multicast_listening();
    }
}
