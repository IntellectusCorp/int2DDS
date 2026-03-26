#![allow(dead_code)]
#![allow(unused_variables)]

use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::common::types::DomainId;
use crate::rtps::entities::entity::Entity;
use crate::rtps::entities::participant::Participant;
use crate::rtps::logic::user_logic::UserLogic;
use crate::rtps::messages::message_receiver::MessageReceiver;
use crate::rtps::transport::socket::MAX_EVENTS;
use crate::rtps::transport::udp::udp_listener::UdpListener;
use crate::rtps::transport::TransportSender;
use mio::{Events, Interest, Poll, Token};
use std::sync::Arc;
use std::time::Duration;

pub(crate) struct UserMulticastListeningTask {
    guid_prefix: GuidPrefix,
    domain_id: DomainId,
    user_multicast_listener: Option<UdpListener>,
    user_logic: Arc<Option<UserLogic>>,
}

impl UserMulticastListeningTask {
    pub(crate) fn new(
        user_multicast_listener: Option<UdpListener>,
        participant: Arc<Participant>,
        sender: Arc<TransportSender>,
    ) -> Self {
        let guid_prefix = { participant.guid().prefix() };
        let domain_id = { participant.domain_id() };
        let (_, _, user_logic) = participant.get_logics();
        Self { guid_prefix, domain_id, user_multicast_listener, user_logic }
    }

    pub(crate) fn multicast_listening(&mut self) -> std::io::Result<()> {
        log::info!("start user multicast listening");
        let mut poll = Poll::new().unwrap();
        let mut events = Events::with_capacity(MAX_EVENTS);

        let listener: &mut UdpListener = match &mut self.user_multicast_listener {
            Some(listener) => listener,
            None => {
                return Err(std::io::Error::other("user multicast listener task is not set"));
            }
        };
        let user_multicast_token = Token(listener.socket().local_addr().unwrap().port() as usize);
        poll.registry().register(listener.socket(), user_multicast_token, Interest::READABLE)?;

        loop {
            match poll.poll(&mut events, Some(Duration::from_secs(1))) {
                Ok(()) => {}
                Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => {
                    log::warn!(
                        "Poll interrupted in user multicast listening task, continuing: {}",
                        e
                    );
                    continue;
                }
                Err(e) => return Err(e),
            }
            for event in events.iter() {
                if event.token() == user_multicast_token && event.is_readable() {
                    while let Some((buffer, from_addr)) = listener.get_message() {
                        let mut message_receiver: MessageReceiver =
                            MessageReceiver::new(self.guid_prefix, &from_addr);
                        let rtps_message = message_receiver.init(&buffer);
                        if rtps_message.is_err() {
                            println!("Failed to parse RTPS message");
                            continue;
                        }

                        // self.user_logic.handle_rtps_message(message_receiver);
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
    use crate::rtps::task::user_traffic::user_multicast_listening_task::UserMulticastListeningTask;
    use crate::rtps::transport::socket::Socket;

    #[test]
    #[ignore]
    fn test_user_multicast_receive() {
        let domain_id = 10;
        let mut socket = Socket::new(domain_id); //domain_id 0
                                                 // Create both sender and listener
        socket.create_socket();
        let participant = Arc::new(Participant::new(
            domain_id,
            socket.participant_id(),
            socket.working_ips(),
            None,
        ));

        // socket.close();
        // return;

        //user traffic multicast port : 7401
        //user traffic unicast port : 7411
        let mut user_multicast_listening_task = UserMulticastListeningTask::new(
            socket.user_traffic_multicast_listener(),
            participant,
            socket.sender(),
        );
        let _ = user_multicast_listening_task.multicast_listening();
    }
}
