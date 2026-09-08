//! Which addresses an announcement actually goes to, and how that list settles.
//!
//! A host given the wildcard port expands into one candidate per participant slot,
//! so most candidates are addresses nobody answers. Announcing to all of them
//! forever would be waste that never ends, so an unanswered candidate is asked
//! less and less often and is eventually dropped: what remains is the set of
//! peers that exist. An address the operator wrote down with a real port is not
//! guesswork and is kept as declared.
//!
//! Dropping a candidate does not make that address unreachable. A participant
//! that appears there later announces itself, and the dial gate admits it on the
//! strength of its host, so the list repairs itself without ever growing back.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

/// The shortest gap between two announcements to the same unanswered candidate.
const AGING_BASE: Duration = Duration::from_secs(2);

/// The longest that gap grows to. Past this the candidate is close to being
/// dropped anyway, so stretching further buys nothing.
const AGING_MAX: Duration = Duration::from_secs(16);

/// How long a guessed address is announced to before it is given up on. Long
/// enough to cover a peer that starts a little later, short enough that a host
/// running fewer participants than assumed settles quickly.
pub(crate) const PEER_PRUNE_DELAY: Duration = Duration::from_secs(30);

#[derive(Debug)]
struct Candidate {
    /// Declared with a port, so it is announced to for as long as this
    /// participant lives — neither slowed down nor dropped.
    pinned: bool,
    /// A participant was reached here, which settles the address as real.
    confirmed: bool,
    next_attempt: Instant,
    interval: Duration,
    give_up_at: Option<Instant>,
}

impl Candidate {
    fn new(pinned: bool, now: Instant, prune_delay: Option<Duration>) -> Self {
        Self {
            pinned,
            confirmed: false,
            next_attempt: now,
            interval: AGING_BASE,
            give_up_at: (!pinned).then(|| prune_delay.map(|delay| now + delay)).flatten(),
        }
    }

    fn settled(&self) -> bool {
        self.pinned || self.confirmed
    }
}

/// The announcement list of one participant.
pub(crate) struct AnnounceTargets {
    candidates: HashMap<SocketAddr, Candidate>,
    prune_delay: Option<Duration>,
}

impl AnnounceTargets {
    /// `pinned` addresses are kept as declared; the rest are guesses that have
    /// to earn their place.
    pub(crate) fn new(
        addresses: impl IntoIterator<Item = (SocketAddr, bool)>,
        prune_delay: Option<Duration>,
        now: Instant,
    ) -> Self {
        let mut candidates: HashMap<SocketAddr, Candidate> = HashMap::new();
        for (addr, pinned) in addresses {
            candidates
                .entry(addr)
                .and_modify(|held| held.pinned |= pinned)
                .or_insert_with(|| Candidate::new(pinned, now, prune_delay));
        }
        Self { candidates, prune_delay }
    }

    /// The addresses this announcement round goes to.
    ///
    /// Calling this advances the schedule, so it belongs to the round that is
    /// about to send and not to a caller that only wants to look.
    pub(crate) fn due(&mut self, now: Instant) -> Vec<SocketAddr> {
        self.candidates.retain(|addr, candidate| {
            let expired = !candidate.settled()
                && candidate.give_up_at.is_some_and(|deadline| now >= deadline);
            if expired {
                log::debug!("[TcpTransportPlugin] no participant at {}, dropping it", addr);
            }
            !expired
        });

        let mut due = Vec::with_capacity(self.candidates.len());
        for (addr, candidate) in self.candidates.iter_mut() {
            if candidate.settled() {
                due.push(*addr);
                continue;
            }
            if now < candidate.next_attempt {
                continue;
            }
            due.push(*addr);
            candidate.next_attempt = now + candidate.interval;
            candidate.interval = (candidate.interval * 2).min(AGING_MAX);
        }
        due
    }

    /// Record that a participant was reached at `addr`, and report whether that
    /// is news.
    ///
    /// An address that was not among the guesses is taken in here: a peer may
    /// have settled on a slot past the expansion, or appeared after its
    /// candidate was given up on. Either way it is real, and announcements have
    /// to keep reaching it.
    ///
    /// The return value marks the one moment a guess turns into a known peer,
    /// which is when everything the guessing left behind stops applying.
    pub(crate) fn confirm(&mut self, addr: SocketAddr) -> bool {
        let prune_delay = self.prune_delay;
        let candidate = self
            .candidates
            .entry(addr)
            .or_insert_with(|| Candidate::new(false, Instant::now(), prune_delay));
        let news = !candidate.confirmed;
        candidate.confirmed = true;
        candidate.give_up_at = None;
        news
    }

    /// Take back the confirmation of `addr`, leaving it a guess again.
    ///
    /// A participant that is gone answers nothing, and an address confirmed
    /// once would otherwise be announced to for the rest of this participant's
    /// life. Returning it to the state it had before it was ever reached puts
    /// it back under the aging and the give-up deadline, so it is asked for a
    /// while longer — a peer that is only restarting still answers — and then
    /// dropped. A declared address is not a guess and is left alone.
    pub(crate) fn revoke(&mut self, addr: SocketAddr, now: Instant) {
        let prune_delay = self.prune_delay;
        let Some(candidate) = self.candidates.get_mut(&addr) else {
            return;
        };
        if candidate.pinned {
            return;
        }
        candidate.confirmed = false;
        candidate.next_attempt = now;
        candidate.interval = AGING_BASE;
        candidate.give_up_at = prune_delay.map(|delay| now + delay);
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.candidates.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(port: u16) -> SocketAddr {
        format!("127.0.0.1:{port}").parse().unwrap()
    }

    #[test]
    fn every_candidate_is_tried_on_the_first_round() {
        let start = Instant::now();
        let mut targets =
            AnnounceTargets::new([(addr(7400), false), (addr(7402), false)], None, start);

        assert_eq!(targets.due(start).len(), 2);
    }

    #[test]
    fn an_unanswered_candidate_is_asked_less_and_less_often() {
        let start = Instant::now();
        let mut targets = AnnounceTargets::new([(addr(7400), false)], None, start);

        assert_eq!(targets.due(start).len(), 1);
        assert!(targets.due(start + AGING_BASE / 2).is_empty(), "not due yet");
        assert_eq!(targets.due(start + AGING_BASE).len(), 1);
        assert!(targets.due(start + AGING_BASE * 2).is_empty(), "the gap has doubled");
        assert_eq!(targets.due(start + AGING_BASE * 3).len(), 1);
    }

    #[test]
    fn a_confirmed_candidate_is_announced_to_every_round() {
        let start = Instant::now();
        let mut targets = AnnounceTargets::new([(addr(7400), false)], None, start);
        targets.due(start);
        targets.confirm(addr(7400));

        assert_eq!(targets.due(start).len(), 1);
        assert_eq!(targets.due(start).len(), 1);
    }

    #[test]
    fn an_unanswered_candidate_is_dropped_once_its_delay_is_up() {
        let start = Instant::now();
        let prune = Duration::from_secs(30);
        let mut targets = AnnounceTargets::new([(addr(7400), false)], Some(prune), start);

        assert_eq!(targets.due(start + prune / 2).len(), 1);
        assert!(targets.due(start + prune).is_empty());
        assert_eq!(targets.len(), 0);
    }

    #[test]
    fn a_declared_address_is_never_slowed_down_or_dropped() {
        let start = Instant::now();
        let prune = Duration::from_secs(30);
        let mut targets = AnnounceTargets::new([(addr(7400), true)], Some(prune), start);

        assert_eq!(targets.due(start).len(), 1);
        assert_eq!(targets.due(start).len(), 1, "a declared address is not aged");
        assert_eq!(targets.due(start + prune * 10).len(), 1, "nor dropped");
    }

    #[test]
    fn a_confirmed_candidate_survives_its_deadline() {
        let start = Instant::now();
        let prune = Duration::from_secs(30);
        let mut targets = AnnounceTargets::new([(addr(7400), false)], Some(prune), start);
        targets.confirm(addr(7400));

        assert_eq!(targets.due(start + prune * 10).len(), 1);
    }

    #[test]
    fn confirming_is_news_only_the_first_time() {
        let start = Instant::now();
        let mut targets = AnnounceTargets::new([(addr(7400), false)], None, start);

        assert!(targets.confirm(addr(7400)), "the first sighting is news");
        assert!(!targets.confirm(addr(7400)), "every later one is not");
    }

    #[test]
    fn a_peer_past_the_expansion_is_taken_in() {
        let start = Instant::now();
        let mut targets = AnnounceTargets::new([(addr(7400), false)], None, start);
        targets.confirm(addr(7500));

        let due = targets.due(start);
        assert_eq!(due.len(), 2);
        assert!(due.contains(&addr(7500)));
    }

    #[test]
    fn a_revoked_candidate_is_aged_again_and_then_dropped() {
        let start = Instant::now();
        let prune = Duration::from_secs(30);
        let mut targets = AnnounceTargets::new([(addr(7400), false)], Some(prune), start);
        targets.confirm(addr(7400));
        targets.revoke(addr(7400), start);

        assert_eq!(targets.due(start).len(), 1);
        assert!(targets.due(start + AGING_BASE / 2).is_empty(), "the aging is back");
        assert!(targets.due(start + prune).is_empty());
        assert_eq!(targets.len(), 0, "and the deadline runs again from the revocation");
    }

    #[test]
    fn a_declared_address_is_not_revoked() {
        let start = Instant::now();
        let prune = Duration::from_secs(30);
        let mut targets = AnnounceTargets::new([(addr(7400), true)], Some(prune), start);
        targets.revoke(addr(7400), start);

        assert_eq!(targets.due(start + prune * 10).len(), 1);
    }

    #[test]
    fn a_peer_that_comes_back_is_news_again() {
        let start = Instant::now();
        let mut targets = AnnounceTargets::new([(addr(7400), false)], None, start);
        targets.confirm(addr(7400));
        targets.revoke(addr(7400), start);

        assert!(targets.confirm(addr(7400)), "the peer has to shed what its absence left behind");
    }

    #[test]
    fn a_prune_delay_of_none_keeps_every_candidate() {
        let start = Instant::now();
        let mut targets = AnnounceTargets::new([(addr(7400), false)], None, start);

        assert_eq!(targets.due(start + Duration::from_secs(3600)).len(), 1);
    }
}
