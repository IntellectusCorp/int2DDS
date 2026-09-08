//! How a configured peer is written down, and what it expands to.
//!
//! Every entry is `ip:port`, the one form every transport reads. A real port is
//! a single address the operator vouches for. Port `0` is the wildcard: it says
//! the port is unknown — the normal case, since which slot a participant settles
//! on depends on what else was running when it started — so the entry stands for
//! every slot of the domain, and the transport that dials it decides which ports
//! those are. A transport with no use for a wildcard simply skips it.

use std::net::{IpAddr, SocketAddr};

/// How many participants one host is assumed to hold when the configuration does
/// not say, which is how far a peer named without a port is expanded. Small
/// enough that a host nobody runs on costs little, and well past what one
/// machine normally carries. A participant that settles past it is still
/// reached: it announces itself, and the peer that hears it admits the address.
pub(crate) const DEFAULT_PARTICIPANTS_PER_HOST: u32 = 16;

/// The port that stands for "every slot of the domain" rather than one address.
pub(crate) const WILDCARD_PORT: u16 = 0;

/// One entry of the configured peer list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PeerSpec {
    pub ip: IpAddr,
    /// `None` when the entry was written with the wildcard port, which is what
    /// turns it into a range of candidates rather than a single address.
    pub port: Option<u16>,
}

impl PeerSpec {
    pub(crate) fn new(ip: IpAddr, port: Option<u16>) -> Self {
        Self { ip, port }
    }

    /// The whole domain range on `ip`.
    pub(crate) fn searched(ip: IpAddr) -> Self {
        Self { ip, port: None }
    }

    /// The addresses this entry stands for. An entry with a real port is itself;
    /// a wildcard is `slot(0) ..= slot(slots - 1)`.
    pub(crate) fn expand(&self, slot: impl Fn(u32) -> u16, slots: u32) -> Vec<SocketAddr> {
        match self.port {
            Some(port) => vec![SocketAddr::new(self.ip, port)],
            None => (0..slots).map(|i| SocketAddr::new(self.ip, slot(i))).collect(),
        }
    }
}

impl std::str::FromStr for PeerSpec {
    type Err = String;

    fn from_str(entry: &str) -> Result<Self, Self::Err> {
        let entry = entry.trim();
        entry.parse::<SocketAddr>().map(Self::from).map_err(|_| {
            let hint = if entry.parse::<IpAddr>().is_ok() {
                format!("; write '{entry}:{WILDCARD_PORT}' to search its whole domain range")
            } else {
                String::new()
            };
            format!("'{entry}' is not an ip:port{hint}")
        })
    }
}

impl From<SocketAddr> for PeerSpec {
    fn from(addr: SocketAddr) -> Self {
        match addr.port() {
            WILDCARD_PORT => Self::searched(addr.ip()),
            port => Self::new(addr.ip(), Some(port)),
        }
    }
}

/// Parse a comma-separated peer list. An entry is `ip:port`, with port `0`
/// standing for the whole domain range; anything else is reported and skipped
/// rather than failing the whole list.
pub(crate) fn parse_peer_specs(peers: &str) -> Vec<PeerSpec> {
    peers
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .filter_map(|entry| match entry.parse::<PeerSpec>() {
            Ok(spec) => Some(spec),
            Err(reason) => {
                log::warn!("Ignoring initial peer: {}", reason);
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_entry_carries_either_a_real_port_or_the_wildcard() {
        let specs = parse_peer_specs("192.168.0.5:7400, 192.168.0.6:0 ,,bogus, 192.168.0.7");

        assert_eq!(
            specs,
            vec![
                PeerSpec::new("192.168.0.5".parse().unwrap(), Some(7400)),
                PeerSpec::searched("192.168.0.6".parse().unwrap()),
            ],
            "a host without a port is not an address and is refused"
        );
    }

    #[test]
    fn a_host_without_a_port_is_told_about_the_wildcard() {
        let refused = "192.168.0.6".parse::<PeerSpec>().expect_err("no port");

        assert!(refused.contains("192.168.0.6:0"), "{refused}");
    }

    #[test]
    fn a_pinned_entry_stands_for_itself_alone() {
        let spec = PeerSpec::new("192.168.0.5".parse().unwrap(), Some(7400));

        assert_eq!(
            spec.expand(|i| 100 + i as u16, DEFAULT_PARTICIPANTS_PER_HOST),
            vec!["192.168.0.5:7400".parse().unwrap()]
        );
    }

    #[test]
    fn the_wildcard_stands_for_every_slot_of_the_domain() {
        let spec: PeerSpec = "127.0.0.1:0".parse().unwrap();

        let expanded = spec.expand(|i| 7400 + 2 * i as u16, DEFAULT_PARTICIPANTS_PER_HOST);

        assert_eq!(expanded.len(), DEFAULT_PARTICIPANTS_PER_HOST as usize);
        assert_eq!(expanded[0], "127.0.0.1:7400".parse().unwrap());
        assert_eq!(expanded[1], "127.0.0.1:7402".parse().unwrap());
        assert_eq!(
            expanded[DEFAULT_PARTICIPANTS_PER_HOST as usize - 1],
            format!("127.0.0.1:{}", 7400 + 2 * (DEFAULT_PARTICIPANTS_PER_HOST - 1))
                .parse()
                .unwrap()
        );
    }

    #[test]
    fn the_wildcard_follows_the_configured_slot_count() {
        let spec: PeerSpec = "127.0.0.1:0".parse().unwrap();

        assert_eq!(spec.expand(|i| 7400 + 2 * i as u16, 3).len(), 3);
    }
}
