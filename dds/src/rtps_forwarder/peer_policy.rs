//! Which of this network's participants are allowed across the link.
//!
//! The decision is made on addresses rather than on anything the participant
//! says about itself, because the deployment already decides which machines
//! reach the far network and an address is what expresses that. A participant
//! that is turned away here never reaches the peer forwarder at all, so the cost
//! it would have added to the link is never paid.

use std::net::Ipv4Addr;

/// A single address or a subnet, written `10.1.0.0/24` or `10.1.2.3`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ipv4Prefix {
    network: u32,
    mask: u32,
}

impl Ipv4Prefix {
    pub fn parse(text: &str) -> Result<Self, String> {
        let (address, length) = match text.split_once('/') {
            Some((address, length)) => {
                let length: u32 = length
                    .parse()
                    .map_err(|_| format!("'{}' has a prefix length that is not a number", text))?;
                if length > 32 {
                    return Err(format!("'{}' has a prefix length above 32", text));
                }
                (address, length)
            }
            None => (text, 32),
        };

        let address: Ipv4Addr =
            address.trim().parse().map_err(|_| format!("'{}' is not an IPv4 address", text))?;

        // Shifting a u32 by 32 is undefined, so the whole-address mask is
        // spelled out rather than computed.
        let mask = if length == 0 { 0 } else { u32::MAX << (32 - length) };
        Ok(Self { network: u32::from(address) & mask, mask })
    }

    pub fn contains(&self, address: Ipv4Addr) -> bool {
        u32::from(address) & self.mask == self.network
    }
}

/// The verdict on one participant, kept separate from the reason so the caller
/// can say why it turned somebody away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    Admit,
    Denied,
    OverCapacity,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct PeerPolicy {
    allowed: Vec<Ipv4Prefix>,
    denied: Vec<Ipv4Prefix>,
    max_forwarded: Option<usize>,
}

impl PeerPolicy {
    pub(crate) fn new(
        allowed: Vec<Ipv4Prefix>,
        denied: Vec<Ipv4Prefix>,
        max_forwarded: Option<usize>,
    ) -> Self {
        Self { allowed, denied, max_forwarded }
    }

    /// A participant may advertise one address per interface it holds, and any
    /// of them identifies the same machine. Being denied on one is therefore
    /// enough to be turned away, while being allowed takes only one match.
    ///
    /// `already_forwarded` is how many participants the forwarder carries now, and
    /// `is_renewal` says the participant is one of them: a participant that is
    /// already through must never be evicted by the cap it was admitted under.
    pub(crate) fn judge(
        &self,
        addresses: &[Ipv4Addr],
        already_forwarded: usize,
        is_renewal: bool,
    ) -> Verdict {
        if addresses.iter().any(|address| self.denied.iter().any(|p| p.contains(*address))) {
            return Verdict::Denied;
        }
        if !self.allowed.is_empty()
            && !addresses.iter().any(|address| self.allowed.iter().any(|p| p.contains(*address)))
        {
            return Verdict::Denied;
        }
        match self.max_forwarded {
            Some(max) if !is_renewal && already_forwarded >= max => Verdict::OverCapacity,
            _ => Verdict::Admit,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prefixes(texts: &[&str]) -> Vec<Ipv4Prefix> {
        texts.iter().map(|t| Ipv4Prefix::parse(t).expect("prefix failed to parse")).collect()
    }

    fn address(text: &str) -> Ipv4Addr {
        text.parse().unwrap()
    }

    #[test]
    fn a_bare_address_matches_only_itself() {
        let prefix = Ipv4Prefix::parse("10.1.2.3").unwrap();
        assert!(prefix.contains(address("10.1.2.3")));
        assert!(!prefix.contains(address("10.1.2.4")));
    }

    #[test]
    fn a_subnet_matches_its_whole_range_and_nothing_past_it() {
        let prefix = Ipv4Prefix::parse("10.1.0.0/24").unwrap();
        assert!(prefix.contains(address("10.1.0.0")));
        assert!(prefix.contains(address("10.1.0.255")));
        assert!(!prefix.contains(address("10.1.1.0")));
    }

    #[test]
    fn the_empty_prefix_matches_everything() {
        let prefix = Ipv4Prefix::parse("0.0.0.0/0").unwrap();
        assert!(prefix.contains(address("10.1.2.3")));
        assert!(prefix.contains(address("192.168.0.1")));
    }

    #[test]
    fn malformed_prefixes_are_rejected() {
        assert!(Ipv4Prefix::parse("10.1.0.0/33").is_err());
        assert!(Ipv4Prefix::parse("10.1.0.0/x").is_err());
        assert!(Ipv4Prefix::parse("not-an-address").is_err());
    }

    #[test]
    fn an_empty_policy_admits_everyone() {
        let policy = PeerPolicy::default();
        assert_eq!(policy.judge(&[address("10.9.9.9")], 1000, false), Verdict::Admit);
    }

    #[test]
    fn an_allow_list_turns_away_everything_outside_it() {
        let policy = PeerPolicy::new(prefixes(&["10.1.0.0/24"]), Vec::new(), None);
        assert_eq!(policy.judge(&[address("10.1.0.5")], 0, false), Verdict::Admit);
        assert_eq!(policy.judge(&[address("10.2.0.5")], 0, false), Verdict::Denied);
    }

    #[test]
    fn denial_beats_permission() {
        let policy = PeerPolicy::new(prefixes(&["10.1.0.0/24"]), prefixes(&["10.1.0.5"]), None);
        assert_eq!(policy.judge(&[address("10.1.0.4")], 0, false), Verdict::Admit);
        assert_eq!(policy.judge(&[address("10.1.0.5")], 0, false), Verdict::Denied);
    }

    #[test]
    fn one_allowed_interface_admits_a_multihomed_participant() {
        let policy = PeerPolicy::new(prefixes(&["10.1.0.0/24"]), Vec::new(), None);
        let interfaces = [address("192.168.7.7"), address("10.1.0.9")];
        assert_eq!(policy.judge(&interfaces, 0, false), Verdict::Admit);
    }

    #[test]
    fn one_denied_interface_turns_away_a_multihomed_participant() {
        let policy = PeerPolicy::new(Vec::new(), prefixes(&["192.168.7.0/24"]), None);
        let interfaces = [address("192.168.7.7"), address("10.1.0.9")];
        assert_eq!(policy.judge(&interfaces, 0, false), Verdict::Denied);
    }

    #[test]
    fn the_cap_stops_new_participants_but_never_evicts_admitted_ones() {
        let policy = PeerPolicy::new(Vec::new(), Vec::new(), Some(2));
        assert_eq!(policy.judge(&[address("10.1.0.1")], 1, false), Verdict::Admit);
        assert_eq!(policy.judge(&[address("10.1.0.2")], 2, false), Verdict::OverCapacity);
        assert_eq!(policy.judge(&[address("10.1.0.2")], 2, true), Verdict::Admit);
    }
}
