use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;

use bytes::Bytes;
use flume::{Receiver, RecvTimeoutError};
use log::{debug, info};

use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::common::rtps_error_code::RtpsResult;
use crate::rtps::entities::participant::Participant;
use crate::rtps::logic::message_processor::unicast_message_processor::UnicastMessageProcessor as _;
use crate::rtps::logic::user_logic::UserLogic;
use crate::rtps::messages::message_receiver::MessageReceiver;

// Byte cap bounds the queued payload independent of fragment size: one large
// retransmit burst plus headroom. The count backstop guards the channel itself.
pub(crate) const USER_RECV_QUEUE_BYTE_CAP: usize = 64 * 1024 * 1024;
pub(crate) const USER_RECV_QUEUE_COUNT_BACKSTOP: usize = 65_536;

// Drains the recv channel and runs parse + delivery off the socket thread so the
// socket keeps draining during large fragment bursts.
pub(crate) struct UserTrafficWorker {
    guid_prefix: GuidPrefix,
    participant: Weak<Participant>,
    user_logic: UserLogic,
    rx: Receiver<(Bytes, SocketAddr)>,
    queued_bytes: Arc<AtomicUsize>,
}

impl UserTrafficWorker {
    pub(crate) fn new(
        guid_prefix: GuidPrefix,
        participant: Weak<Participant>,
        user_logic: UserLogic,
        rx: Receiver<(Bytes, SocketAddr)>,
        queued_bytes: Arc<AtomicUsize>,
    ) -> Self {
        Self { guid_prefix, participant, user_logic, rx, queued_bytes }
    }

    pub(crate) fn run(&self) {
        let participant = match self.participant.upgrade() {
            Some(participant) => participant,
            None => {
                debug!("user worker: participant already dropped before start");
                return;
            }
        };

        loop {
            match self.rx.recv_timeout(Duration::from_millis(100)) {
                Ok((bytes, from_addr)) => {
                    self.queued_bytes.fetch_sub(bytes.len(), Ordering::AcqRel);

                    if participant.is_terminated() {
                        debug!("user worker detected termination, stopping");
                        return;
                    }

                    if let Err(e) = self.process(bytes, from_addr) {
                        debug!(
                            "user worker failed to process message from {:?}: {:?}",
                            from_addr, e
                        );
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    if participant.is_terminated() {
                        debug!("user worker detected termination while idle, stopping");
                        return;
                    }
                }
                Err(RecvTimeoutError::Disconnected) => {
                    info!("user worker channel disconnected, stopping");
                    return;
                }
            }
        }
    }

    fn process(&self, bytes: Bytes, from_addr: SocketAddr) -> RtpsResult<()> {
        let mut message_receiver = MessageReceiver::new(self.guid_prefix, &from_addr);
        message_receiver.init(&bytes)?;

        let mut user_logic = self.user_logic.clone();
        user_logic.handle_rtps_message(message_receiver)?;

        Ok(())
    }
}
