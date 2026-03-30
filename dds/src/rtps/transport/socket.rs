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

        let is_network_specified = get_network_interface().is_some() || get_network_ip().is_some();

        if let Ok(Some(ip)) = crate::common::int2dds_feature_ffi::get_working_ip() {
            if is_network_specified {
                ips.push(ip);
                from_feature = true;
            } else {
                log::warn!(
                    "int2DDS-feature is enabled but no network interface specified. \
                            Falling back to default (auto-detection)"
                );
            }
        } else if is_network_specified {
            log::warn!(
                "Env variable INT2DDS_NETWORK_INTERFACE or INT2DDS_NETWORK_IP is set \
                 but int2DDS-feature is not enabled. Ignoring the value"
            );
        }

        if ips.is_empty() {
            if let Ok(ifaces) = get_if_addrs::get_if_addrs() {
                for iface in ifaces {
                    if !iface.ip().is_loopback() {
                        ips.push(iface.ip().to_string());
                    }
                }
            }
        }

        let use_loopback = crate::common::env::get_use_loopback_interface();
        let should_add_loopback =
            !from_feature && !ips.contains(&"127.0.0.1".to_string()) && use_loopback;

        if ips.is_empty() || should_add_loopback {
            ips.push("127.0.0.1".to_string());
        }

        Ok(WorkingIps { ips, from_feature })
    }

    pub(crate) fn working_ips(&self) -> Vec<String> {
        self.working_ips.ips.clone()
    }

    pub(crate) fn is_working_ips_from_feature(&self) -> bool {
        self.working_ips.from_feature
    }

    pub(crate) fn get_sender_bind_addr(&self) -> String {
        if self.working_ips.from_feature {
            return self.working_ips.ips[0].clone();
        }
        let only_loopback =
            self.working_ips.ips.len() == 1 && self.working_ips.ips[0] == "127.0.0.1";
        if only_loopback {
            "127.0.0.1".to_string()
        } else {
            "0.0.0.0".to_string()
        }
    }

    pub(crate) fn get_sender_multicast_if_addr(&self) -> String {
        if self.working_ips.from_feature {
            return self.working_ips.ips[0].clone();
        }

        if let Ok(addr) = std::net::UdpSocket::bind("0.0.0.0:0")
            .and_then(|s| s.connect("8.8.8.8:80").map(|_| s))
            .and_then(|s| s.local_addr())
        {
            let ip = addr.ip().to_string();
            if ip != "127.0.0.1" && ip != "0.0.0.0" {
                return ip;
            }
        }

        self.working_ips
            .ips
            .iter()
            .find(|ip| ip.as_str() != "127.0.0.1")
            .cloned()
            .unwrap_or_else(|| "127.0.0.1".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        )
        .unwrap();
        let transport: Arc<dyn TransportPlugin> = Arc::from(transport);
        socket.set_transport(transport);
        assert!(socket.transport.is_some());
    }
}
