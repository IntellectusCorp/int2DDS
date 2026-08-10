//! What the link actually carries.
//!
//! The relay lets every participant discover every participant on the far
//! network, so discovery cost grows with the number of participants while
//! sample cost does not. Metatraffic and user data are therefore counted apart:
//! their ratio is what says how many participants this design can carry across
//! a wide area link.

use std::sync::atomic::{AtomicU64, Ordering};

use super::link::Channel;

/// A reading taken at one instant. Counters are sampled one after another, so a
/// reading taken under load is close rather than exact.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct LinkStats {
    pub sent_metatraffic_frames: u64,
    pub sent_metatraffic_bytes: u64,
    pub sent_user_data_frames: u64,
    pub sent_user_data_bytes: u64,
    pub received_metatraffic_frames: u64,
    pub received_metatraffic_bytes: u64,
    pub received_user_data_frames: u64,
    pub received_user_data_bytes: u64,
}

impl LinkStats {
    pub fn metatraffic_bytes(&self) -> u64 {
        self.sent_metatraffic_bytes + self.received_metatraffic_bytes
    }

    pub fn user_data_bytes(&self) -> u64 {
        self.sent_user_data_bytes + self.received_user_data_bytes
    }
}

#[derive(Default)]
pub(crate) struct LinkCounters {
    sent_metatraffic_frames: AtomicU64,
    sent_metatraffic_bytes: AtomicU64,
    sent_user_data_frames: AtomicU64,
    sent_user_data_bytes: AtomicU64,
    received_metatraffic_frames: AtomicU64,
    received_metatraffic_bytes: AtomicU64,
    received_user_data_frames: AtomicU64,
    received_user_data_bytes: AtomicU64,
}

impl LinkCounters {
    /// Counted once a datagram is committed to the link, not when it is offered,
    /// so a full queue does not inflate the reading.
    pub(crate) fn count_sent(&self, channel: Channel, payload_len: usize) {
        let (frames, bytes) = match channel {
            Channel::Metatraffic => (&self.sent_metatraffic_frames, &self.sent_metatraffic_bytes),
            Channel::UserData => (&self.sent_user_data_frames, &self.sent_user_data_bytes),
        };
        frames.fetch_add(1, Ordering::Relaxed);
        bytes.fetch_add(payload_len as u64, Ordering::Relaxed);
    }

    pub(crate) fn count_received(&self, channel: Channel, payload_len: usize) {
        let (frames, bytes) = match channel {
            Channel::Metatraffic => {
                (&self.received_metatraffic_frames, &self.received_metatraffic_bytes)
            }
            Channel::UserData => (&self.received_user_data_frames, &self.received_user_data_bytes),
        };
        frames.fetch_add(1, Ordering::Relaxed);
        bytes.fetch_add(payload_len as u64, Ordering::Relaxed);
    }

    pub(crate) fn read(&self) -> LinkStats {
        LinkStats {
            sent_metatraffic_frames: self.sent_metatraffic_frames.load(Ordering::Relaxed),
            sent_metatraffic_bytes: self.sent_metatraffic_bytes.load(Ordering::Relaxed),
            sent_user_data_frames: self.sent_user_data_frames.load(Ordering::Relaxed),
            sent_user_data_bytes: self.sent_user_data_bytes.load(Ordering::Relaxed),
            received_metatraffic_frames: self.received_metatraffic_frames.load(Ordering::Relaxed),
            received_metatraffic_bytes: self.received_metatraffic_bytes.load(Ordering::Relaxed),
            received_user_data_frames: self.received_user_data_frames.load(Ordering::Relaxed),
            received_user_data_bytes: self.received_user_data_bytes.load(Ordering::Relaxed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_direction_and_channel_is_counted_apart() {
        let counters = LinkCounters::default();
        counters.count_sent(Channel::Metatraffic, 100);
        counters.count_sent(Channel::Metatraffic, 40);
        counters.count_sent(Channel::UserData, 500);
        counters.count_received(Channel::Metatraffic, 60);

        let stats = counters.read();
        assert_eq!(stats.sent_metatraffic_frames, 2);
        assert_eq!(stats.sent_metatraffic_bytes, 140);
        assert_eq!(stats.sent_user_data_frames, 1);
        assert_eq!(stats.sent_user_data_bytes, 500);
        assert_eq!(stats.received_metatraffic_bytes, 60);
        assert_eq!(stats.received_user_data_frames, 0);
        assert_eq!(stats.metatraffic_bytes(), 200);
        assert_eq!(stats.user_data_bytes(), 500);
    }
}
