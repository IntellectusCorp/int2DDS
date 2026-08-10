//! Configuration of one side of a gateway pair.

use std::{
    io,
    net::{Ipv4Addr, SocketAddr},
};

use crate::rtps::transport::port_manager::PortManager;

/// Which end of the link this gateway is. Exactly one of the pair listens.
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
    pub link: LinkRole,
}

impl RelayConfig {
    pub fn new(
        lan_domain_id: u32,
        metatraffic_port: u16,
        user_data_port: u16,
        link: LinkRole,
    ) -> Self {
        Self { lan_domain_id, lan_ip: None, metatraffic_port, user_data_port, link }
    }

    pub fn with_lan_ip(mut self, lan_ip: Ipv4Addr) -> Self {
        self.lan_ip = Some(lan_ip);
        self
    }

    pub(crate) fn resolve(&self) -> io::Result<ResolvedConfig> {
        if self.metatraffic_port == self.user_data_port {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "metatraffic and user data ports must differ",
            ));
        }

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
            link: self.link.clone(),
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
    pub(crate) link: LinkRole,
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
