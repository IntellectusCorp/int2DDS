//! Configuration of one side of a gateway pair.

use std::{
    io,
    net::{Ipv4Addr, SocketAddr},
};

use crate::rtps::transport::port_manager::PortManager;

use super::peer_policy::{Ipv4Prefix, PeerPolicy};

/// Which end of one link this gateway is. Exactly one end of each pair listens.
#[derive(Debug, Clone)]
pub enum LinkRole {
    Listen(SocketAddr),
    Connect(SocketAddr),
}

#[derive(Debug, Clone)]
pub struct RelayConfig {
    /// Domain the gateway's own network runs on. Announcements arriving from
    /// the peer gateway are retargeted at this domain.
    pub lan_domain_id: u32,
    /// Address the gateway advertises to its network. Auto-detected when unset.
    pub lan_ip: Option<Ipv4Addr>,
    /// Port that stands in for every relayed participant's metatraffic locator.
    pub metatraffic_port: u16,
    /// Port that stands in for every relayed participant's user data locator.
    pub user_data_port: u16,
    /// One entry per peer gateway. Each is an independent link: nothing that
    /// arrives on one is ever passed to another, so the gateways form a star
    /// around each network rather than a routed mesh.
    pub links: Vec<LinkRole>,
    /// Addresses of this network's participants that may be carried across the
    /// link. An empty list carries every participant.
    pub allowed_peers: Vec<String>,
    /// Addresses that are never carried, weighed before the allowed list.
    pub denied_peers: Vec<String>,
    /// Guard against a misconfigured network flooding the link. It bounds how
    /// many participants are relayed at once and is not a way of choosing
    /// between them: which participants cross is what the lists decide.
    pub max_relayed_participants: Option<usize>,
}

impl RelayConfig {
    pub fn new(
        lan_domain_id: u32,
        metatraffic_port: u16,
        user_data_port: u16,
        link: LinkRole,
    ) -> Self {
        Self {
            lan_domain_id,
            lan_ip: None,
            metatraffic_port,
            user_data_port,
            links: vec![link],
            allowed_peers: Vec::new(),
            denied_peers: Vec::new(),
            max_relayed_participants: None,
        }
    }

    pub fn with_lan_ip(mut self, lan_ip: Ipv4Addr) -> Self {
        self.lan_ip = Some(lan_ip);
        self
    }

    /// Adds another peer gateway to relay with.
    pub fn with_peer(mut self, link: LinkRole) -> Self {
        self.links.push(link);
        self
    }

    pub fn with_allowed_peers<I, S>(mut self, prefixes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.allowed_peers.extend(prefixes.into_iter().map(Into::into));
        self
    }

    pub fn with_denied_peers<I, S>(mut self, prefixes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.denied_peers.extend(prefixes.into_iter().map(Into::into));
        self
    }

    pub fn with_max_relayed_participants(mut self, max: usize) -> Self {
        self.max_relayed_participants = Some(max);
        self
    }

    pub(crate) fn resolve(&self) -> io::Result<ResolvedConfig> {
        if self.metatraffic_port == self.user_data_port {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "metatraffic and user data ports must differ",
            ));
        }

        if self.links.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a relay needs at least one peer gateway",
            ));
        }

        // Parsed here so that a typo stops the gateway from starting instead
        // of silently turning participants away at run time.
        let policy = PeerPolicy::new(
            parse_prefixes(&self.allowed_peers)?,
            parse_prefixes(&self.denied_peers)?,
            self.max_relayed_participants,
        );

        let lan_ip = match self.lan_ip {
            Some(ip) => ip,
            None => detect_lan_ip()?,
        };

        Ok(ResolvedConfig {
            lan_domain_id: self.lan_domain_id,
            lan_ip,
            metatraffic_port: self.metatraffic_port,
            user_data_port: self.user_data_port,
            discovery_multicast_port: PortManager::get_discovery_traffic_multicast_port(
                self.lan_domain_id,
            ),
            links: self.links.clone(),
            policy,
        })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedConfig {
    pub(crate) lan_domain_id: u32,
    pub(crate) lan_ip: Ipv4Addr,
    pub(crate) metatraffic_port: u16,
    pub(crate) user_data_port: u16,
    pub(crate) discovery_multicast_port: u16,
    pub(crate) links: Vec<LinkRole>,
    pub(crate) policy: PeerPolicy,
}

fn parse_prefixes(texts: &[String]) -> io::Result<Vec<Ipv4Prefix>> {
    texts
        .iter()
        .map(|text| {
            Ipv4Prefix::parse(text).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))
        })
        .collect()
}

/// First routable IPv4 address of the host. The gateway has to advertise an
/// address its own network can reach, so loopback is only a last resort.
fn detect_lan_ip() -> io::Result<Ipv4Addr> {
    let interfaces = if_addrs::get_if_addrs()?;

    let routable = interfaces.iter().find_map(|interface| match interface.ip() {
        std::net::IpAddr::V4(ip) if !ip.is_loopback() && !ip.is_link_local() => Some(ip),
        _ => None,
    });
    if let Some(ip) = routable {
        return Ok(ip);
    }

    interfaces
        .iter()
        .find_map(|interface| match interface.ip() {
            std::net::IpAddr::V4(ip) => Some(ip),
            _ => None,
        })
        .ok_or_else(|| io::Error::new(io::ErrorKind::AddrNotAvailable, "no IPv4 interface found"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> RelayConfig {
        RelayConfig::new(2, 7500, 7501, LinkRole::Listen("127.0.0.1:9000".parse().unwrap()))
            .with_lan_ip(Ipv4Addr::new(10, 1, 2, 3))
    }

    #[test]
    fn resolve_derives_the_domain_multicast_port() {
        let resolved = config().resolve().expect("resolve failed");
        assert_eq!(resolved.discovery_multicast_port, 7900);
        assert_eq!(resolved.lan_ip, Ipv4Addr::new(10, 1, 2, 3));
    }

    #[test]
    fn every_configured_peer_survives_resolution() {
        let config = config().with_peer(LinkRole::Connect("127.0.0.1:9001".parse().unwrap()));
        assert_eq!(config.resolve().expect("resolve failed").links.len(), 2);
    }

    #[test]
    fn resolve_rejects_a_relay_without_a_peer() {
        let mut config = config();
        config.links.clear();
        assert!(config.resolve().is_err());
    }

    #[test]
    fn resolve_rejects_a_malformed_peer_prefix() {
        assert!(config().with_allowed_peers(["10.1.0.0/33"]).resolve().is_err());
    }

    #[test]
    fn resolve_rejects_a_shared_port() {
        let mut config = config();
        config.user_data_port = config.metatraffic_port;
        assert!(config.resolve().is_err());
    }

    #[test]
    fn detect_lan_ip_finds_an_address() {
        assert!(detect_lan_ip().is_ok());
    }
}
