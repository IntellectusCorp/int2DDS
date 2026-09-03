use std::net::Ipv4Addr;
use std::sync::Arc;
use std::vec;

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

        // Check if user specified which network to use via env variable
        let is_network_specified = get_network_interface().is_some() || get_network_ip().is_some();

        if let Some(ip) = crate::common::enterprise_hooks::call_resolve_ip() {
            // enterprise resolve_ip hook enabled
            if is_network_specified {
                log::debug!("Using enterprise hooks specified IP: {}", ip);
                ips.push(ip);
                from_feature = true;
            } else {
                log::warn!(
                    "enterprise resolve_ip hook is enabled but no network interface specified. \
                            Falling back to default (auto-detection)"
                );
            }
        } else if is_network_specified {
            // These variables can only be used with the enterprise resolve_ip hook
            log::warn!(
                "Env variable INT2DDS_NETWORK_INTERFACE or INT2DDS_NETWORK_IP is set \
                 but the enterprise resolve_ip hook is not enabled. Ignoring the value"
            );
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
        // also skipped when from_feature — feature-specified NIC takes full control
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
        // From enterprise hooks: bind to the hook-specified IP directly
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
    pub(crate) fn get_sender_multicast_if_addr(&self, configured_if: Option<Ipv4Addr>) -> String {
        // A per-participant property beats every process-wide source below it.
        if let Some(ip) = configured_if {
            log::debug!("Using property-configured multicast interface IP: {}", ip);
            return ip.to_string();
        }

        // From enterprise hooks: use the hook-specified IP directly
        if self.working_ips.from_feature {
            log::debug!(
                "Using enterprise hooks specified multicast interface IP: {}",
                self.working_ips.ips[0]
            );
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
            socket.get_sender_multicast_if_addr(None),
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
}
