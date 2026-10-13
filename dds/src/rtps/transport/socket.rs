use std::net::IpAddr;
use std::sync::Arc;
use std::vec;

use network_interface::{NetworkInterface, NetworkInterfaceConfig};

use crate::common::env::{get_network_interface, get_network_ip};
use crate::rtps::common::types::{DomainId, ParticipantId};
use crate::rtps::transport::plugin::TransportPlugin;

/// Network configuration and transport holder.
///
/// Socket resolves working IPs and participant_id, then receives
/// a pre-created TransportPlugin from DcpsBridge via `set_transport()`.
pub(crate) struct Socket {
    transport: Option<Arc<dyn TransportPlugin>>,

    domain_id: DomainId,
    participant_id: ParticipantId,
    working_ips: WorkingIps,
}

#[derive(Debug, Clone)]
pub(crate) struct WorkingIps {
    pub ips: Vec<String>,
    pub from_feature: bool,
}

pub const MAX_EVENTS: usize = 512;

impl Socket {
    pub(crate) fn new(domain_id: DomainId) -> Self {
        let working_ip = Self::get_new_working_ips().unwrap_or_else(|e| {
            log::error!("[socket] Failed to determine working IP: {}. Using fallback 127.0.0.1", e);
            WorkingIps { ips: vec!["127.0.0.1".to_string()], from_feature: false }
        });
        Self { transport: None, domain_id, participant_id: 0, working_ips: working_ip }
    }

    pub(crate) fn participant_id(&self) -> ParticipantId {
        self.participant_id
    }

    // ── Transport (injected by DcpsBridge) ───────────────────────────

    /// Store the transport plugin created by DcpsBridge.
    pub(crate) fn set_transport(&mut self, transport: Arc<dyn TransportPlugin>) {
        self.transport = Some(transport);
    }

    /// Get a shared reference to the transport plugin.
    pub(crate) fn transport(&self) -> Arc<dyn TransportPlugin> {
        self.transport.clone().expect("[socket] transport is not set")
    }

    // ── Close ───────────────────────────────────────────────────────

    pub(crate) fn close(&mut self) {
        if let Some(transport) = self.transport.take() {
            transport.close();
        }
        log::info!("[socket] all resources closed");
    }

    // ── Working IP helpers ──────────────────────────────────────────

    fn get_new_working_ips() -> std::io::Result<WorkingIps> {
        let mut ips: Vec<String> = Vec::new();
        let mut from_feature = false;

        // INT2DDS_NETWORK_IP first, then INT2DDS_NETWORK_INTERFACE; a value that
        // cannot be used is ignored and every NIC is used as if it were unset.
        if let Some(value) = get_network_ip() {
            match value.parse::<IpAddr>() {
                Ok(ip) if ip.is_ipv4() && !ip.is_loopback() => {
                    log::debug!("Using INT2DDS_NETWORK_IP: {}", ip);
                    ips.push(ip.to_string());
                    from_feature = true;
                }
                _ => log::warn!(
                    "INT2DDS_NETWORK_IP '{}' is not a non-loopback IPv4 address; ignoring it",
                    value
                ),
            }
        }
        if !from_feature {
            if let Some(name) = get_network_interface() {
                let found = NetworkInterface::show()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|iface| iface.name == name)
                    .flat_map(|iface| iface.addr)
                    .map(|addr| addr.ip())
                    .find(|ip| ip.is_ipv4() && !ip.is_loopback());
                match found {
                    Some(ip) => {
                        log::debug!("Using INT2DDS_NETWORK_INTERFACE '{}': {}", name, ip);
                        ips.push(ip.to_string());
                        from_feature = true;
                    }
                    None => log::warn!(
                        "INT2DDS_NETWORK_INTERFACE '{}' has no non-loopback IPv4 address; \
                         using all interfaces",
                        name
                    ),
                }
            }
        }

        let use_loopback = crate::common::env::get_use_loopback_interface();

        // If no specific IP was selected, use all available NICs.
        // Some environments, notably WSL, attach non-127/8 addresses to `lo`.
        // Treat those as loopback-interface addresses unless loopback use is explicit.
        if ips.is_empty() {
            if let Ok(ifaces) = if_addrs::get_if_addrs() {
                for iface in ifaces {
                    let is_loopback_interface = Self::is_loopback_interface_name(&iface.name);
                    if is_loopback_interface && !use_loopback {
                        log::debug!(
                            "Skipping loopback interface IP while loopback is disabled: {} ({})",
                            iface.ip(),
                            iface.name
                        );
                        continue;
                    }
                    if !iface.ip().is_loopback() {
                        ips.push(iface.ip().to_string());
                        log::debug!("Adding IP from NIC: {}", iface.ip());
                    }
                }
            }
        }

        let should_add_loopback =
            !from_feature && !ips.contains(&"127.0.0.1".to_string()) && use_loopback;

        // If no NIC available or loopback is set to use, add localhost IP to the list
        // also skipped when an address is pinned by INT2DDS_NETWORK_IP / _INTERFACE
        if ips.is_empty() || should_add_loopback {
            ips.push("127.0.0.1".to_string());
        }

        log::debug!("Working IPs determined: {:?}, from_feature: {}", ips, from_feature);

        Ok(WorkingIps { ips, from_feature })
    }

    pub(crate) fn working_ips(&self) -> Vec<String> {
        self.working_ips.ips.clone()
    }

    pub(crate) fn is_working_ips_from_feature(&self) -> bool {
        self.working_ips.from_feature
    }

    pub(crate) fn get_sender_bind_addr(&self) -> String {
        // Pinned by INT2DDS_NETWORK_IP / _INTERFACE: bind to that address directly
        if self.working_ips.from_feature {
            return self.working_ips.ips[0].clone();
        }
        // This happens when no physical NIC exists
        let only_loopback =
            self.working_ips.ips.len() == 1 && self.working_ips.ips[0] == "127.0.0.1";
        if only_loopback {
            // No physical NIC available, 0.0.0.0 has no interface to route through
            log::debug!("Only loopback interface is available, binding sender to 127.0.0.1");
            "127.0.0.1".to_string()
        } else {
            // 0.0.0.0 allows unicast to reach any subnet via OS routing table
            log::debug!("Binding sender to 0.0.0.0");
            "0.0.0.0".to_string()
        }
    }

    // TODO: Ideally, create one multicast sender per NIC with set_multicast_if_v4(ip) + bind(ip:0)
    // to send multicast out of all NICs simultaneously.
    // 0.0.0.0 relies on default route, which doesn't exist in gateway-less environments,
    // and multicast addresses (e.g. 239.x) don't match any subnet route.
    pub(crate) fn get_sender_multicast_if_addr(&self) -> String {
        // Pinned by INT2DDS_NETWORK_IP / _INTERFACE: use that address directly
        if self.working_ips.from_feature {
            log::debug!("Using pinned multicast interface IP: {}", self.working_ips.ips[0]);
            return self.working_ips.ips[0].clone();
        }

        // loopback-only multicast egress when opted in.
        if crate::common::env::get_force_loopback_multicast() {
            log::debug!("FORCE_LOOPBACK_MULTICAST: forcing multicast interface IP to 127.0.0.1");
            return "127.0.0.1".to_string();
        }

        // Probe the OS routing table by connecting to a public address.
        // Resolve the default outgoing IP via 0.0.0.0 bind & connect
        if let Ok(addr) = std::net::UdpSocket::bind("0.0.0.0:0")
            .and_then(|s| s.connect("8.8.8.8:80").map(|_| s))
            .and_then(|s| s.local_addr())
        {
            let ip = addr.ip().to_string();
            if ip != "127.0.0.1" && ip != "0.0.0.0" {
                log::debug!("Resolved default outgoing multicast interface IP: {}", ip);
                return ip;
            }
        }

        // No default route (e.g. direct Ethernet without gateway):
        // pick the first non-loopback IP from working_ips
        let chosen_ip = self
            .working_ips
            .ips
            .iter()
            .find(|ip| ip.as_str() != "127.0.0.1")
            .cloned()
            .unwrap_or_else(|| {
                log::debug!("No suitable multicast interface found, using loopback");
                "127.0.0.1".to_string()
            }); // Fallback to loopback if no other IPs are available

        log::debug!("Using multicast interface IP chosen from working IPs: {}", chosen_ip);
        chosen_ip
    }

    fn is_loopback_interface_name(name: &str) -> bool {
        name == "lo" || name.starts_with("lo:")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dcps::infrastructure::qos_policy::PropertyQosPolicy;
    use crate::rtps::transport::plugin::TransportPluginFactory;
    use crate::rtps::transport::TransportType;

    #[test]
    fn test_create_socket() {
        let mut socket = Socket::new(0);
        let transport = TransportPluginFactory::create(
            TransportType::UDP,
            0,
            socket.participant_id(),
            socket.get_sender_bind_addr(),
            socket.get_sender_multicast_if_addr(),
            socket.working_ips(),
            [0u8; 12],
            None,
            &PropertyQosPolicy::default(),
        )
        .unwrap();
        let transport: Arc<dyn TransportPlugin> = Arc::from(transport);
        socket.set_transport(transport);
        assert!(socket.transport.is_some());
    }

    #[test]
    fn loopback_interface_name_matches_linux_loopback_only() {
        assert!(Socket::is_loopback_interface_name("lo"));
        assert!(Socket::is_loopback_interface_name("lo:0"));
        assert!(!Socket::is_loopback_interface_name("eth0"));
        assert!(!Socket::is_loopback_interface_name("wlo1"));
    }

    use std::sync::{Mutex, MutexGuard};

    // Serialize env-mutating tests since std::env is process-global
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Holds the lock and clears both selection variables on drop, even after a panic.
    struct NetworkEnv(#[allow(dead_code)] MutexGuard<'static, ()>);

    impl NetworkEnv {
        fn set(ip: Option<&str>, interface: Option<&str>) -> Self {
            let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            clear_network_env();
            if let Some(v) = ip {
                crate::common::env::set_network_ip(v);
            }
            if let Some(v) = interface {
                crate::common::env::set_network_interface(v);
            }
            Self(guard)
        }
    }

    impl Drop for NetworkEnv {
        fn drop(&mut self) {
            clear_network_env();
        }
    }

    fn clear_network_env() {
        unsafe {
            std::env::remove_var("INT2DDS_NETWORK_IP");
            std::env::remove_var("INT2DDS_NETWORK_INTERFACE");
        }
    }

    fn working_ips_with(ip: Option<&str>, interface: Option<&str>) -> WorkingIps {
        let _env = NetworkEnv::set(ip, interface);
        Socket::get_new_working_ips().unwrap()
    }

    #[test]
    fn network_ip_pins_a_usable_ipv4() {
        let w = working_ips_with(Some("192.0.2.10"), None);
        assert!(w.from_feature, "{w:?}");
        assert_eq!(w.ips, vec!["192.0.2.10".to_string()]);
    }

    #[test]
    fn network_ip_ignores_loopback() {
        assert!(!working_ips_with(Some("127.0.0.1"), None).from_feature);
    }

    #[test]
    fn network_ip_ignores_a_malformed_value() {
        assert!(!working_ips_with(Some("not-an-ip"), None).from_feature);
    }

    #[test]
    fn network_ip_ignores_ipv6() {
        assert!(!working_ips_with(Some("fe80::1"), None).from_feature);
    }

    #[test]
    fn network_interface_ignores_an_unknown_name() {
        assert!(!working_ips_with(None, Some("int2dds-no-such-nic")).from_feature);
    }

    #[test]
    fn network_ip_takes_precedence_over_interface() {
        let w = working_ips_with(Some("192.0.2.10"), Some("int2dds-no-such-nic"));
        assert_eq!(w.ips, vec!["192.0.2.10".to_string()]);
    }

    #[test]
    fn network_interface_pins_an_ipv4_of_that_interface() {
        use network_interface::{NetworkInterface, NetworkInterfaceConfig};
        // Picks a NIC from this host, so it holds on any CI runner; no usable NIC, nothing to check.
        let ifaces = NetworkInterface::show().unwrap_or_default();
        let usable = |ip: &std::net::IpAddr| ip.is_ipv4() && !ip.is_loopback();
        let Some(name) =
            ifaces.iter().find(|i| i.addr.iter().any(|a| usable(&a.ip()))).map(|i| i.name.clone())
        else {
            return;
        };
        let candidates: Vec<String> = ifaces
            .iter()
            .filter(|i| i.name == name)
            .flat_map(|i| i.addr.iter().map(|a| a.ip()))
            .filter(usable)
            .map(|ip| ip.to_string())
            .collect();
        let w = working_ips_with(None, Some(&name));
        assert!(w.from_feature, "interface {name} was not pinned: {w:?}");
        assert_eq!(w.ips.len(), 1, "{w:?}");
        assert!(
            candidates.contains(&w.ips[0]),
            "{} is not an IPv4 of {name}: {candidates:?}",
            w.ips[0]
        );

        // With a real NIC as well, INT2DDS_NETWORK_IP still wins and nothing is added.
        let w = working_ips_with(Some("192.0.2.10"), Some(&name));
        assert_eq!(w.ips, vec!["192.0.2.10".to_string()]);
    }
}
