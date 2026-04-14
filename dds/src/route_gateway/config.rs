//! Configuration loading for the Route Gateway binary.
//!
//! Loads a JSON file describing the LocalNode (LAN side) and RemoteNode
//! (WAN side) participants and the topic filter applied by AutoRelay.

use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::{
    core::error::{DdsError, DdsResult},
    domain::qos::DomainParticipantQos,
    infrastructure::qos_policy::PropertyQosPolicy,
};

/// TLS settings for one side of a Route Gateway.
///
/// All three PEM file paths (`ca_file`, `cert_file`, `key_file`) must be
/// provided together; omitting the entire block disables TLS for that node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsNodeConfig {
    /// Path to a PEM file containing one or more trusted CA certificates.
    pub ca_file: String,
    /// Path to a PEM file containing this peer's certificate chain.
    pub cert_file: String,
    /// Path to a PEM file containing this peer's private key.
    pub key_file: String,
    /// SNI server name sent during TLS handshake (client side).
    /// Defaults to `"localhost"` when omitted.
    #[serde(default = "default_server_name")]
    pub server_name: String,
    /// Require the remote peer to present a certificate (mutual TLS).
    /// Defaults to `false`.
    #[serde(default)]
    pub verify_peer: bool,
}

fn default_server_name() -> String {
    "localhost".to_string()
}

/// Configuration for one side of a Route Gateway.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    pub domain_id: i32,
    /// Transport for this participant. Accepted values: "udp", "tcp", "hybrid", "shm".
    ///
    /// **Supported WAN transport: TCP only.**
    /// LocalNode (LAN side) typically uses "udp"; RemoteNode (WAN side) must use "tcp"
    /// in the current implementation. Other values are accepted for completeness but
    /// are not validated against the WAN scenario.
    pub transport: String,
    /// Optional list of initial peers (used by TCP transport to reach the remote gateway).
    /// Format per entry: `"<ip>:<port>"`. Port follows the RTPS rule
    /// `7400 + 250 * domain_id` unless `INT2DDS_TCP_PORT` is set.
    #[serde(default)]
    pub initial_peers: Vec<String>,
    /// Optional TLS configuration. When present, TCP connections are wrapped
    /// in TLS. When absent, plain TCP is used.
    #[serde(default)]
    pub tls: Option<TlsNodeConfig>,
}

impl NodeConfig {
    pub fn to_participant_qos(&self) -> DomainParticipantQos {
        let mut property = PropertyQosPolicy::default();
        property.set("int2dds.transport", &self.transport);
        if !self.initial_peers.is_empty() {
            property.set("int2dds.initial_peers", &self.initial_peers.join(","));
        }
        if let Some(tls) = &self.tls {
            property.set("int2dds.tls.ca_file", &tls.ca_file);
            property.set("int2dds.tls.cert_file", &tls.cert_file);
            property.set("int2dds.tls.key_file", &tls.key_file);
            property.set("int2dds.tls.server_name", &tls.server_name);
            property.set(
                "int2dds.tls.verify_peer",
                if tls.verify_peer { "true" } else { "false" },
            );
        }
        DomainParticipantQos { property, ..Default::default() }
    }
}

/// Topic filter applied to AutoRelay. Defaults to "*".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoRelayConfig {
    #[serde(default = "default_filter")]
    pub filter: String,
}

fn default_filter() -> String {
    "*".to_string()
}

impl Default for AutoRelayConfig {
    fn default() -> Self {
        Self { filter: default_filter() }
    }
}

/// Top-level Route Gateway configuration file format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteGatewayConfig {
    pub local: NodeConfig,
    pub remote: NodeConfig,
    #[serde(default)]
    pub auto_relay: AutoRelayConfig,
    /// Polling period in milliseconds. Default: 50ms.
    #[serde(default = "default_poll_period_ms")]
    pub poll_period_ms: u64,
}

fn default_poll_period_ms() -> u64 {
    50
}

impl RouteGatewayConfig {
    pub fn from_file(path: impl AsRef<Path>) -> DdsResult<Self> {
        let raw = fs::read_to_string(path.as_ref())
            .map_err(|e| DdsError::Error(format!("failed to read config: {e}")))?;
        Self::from_json(&raw)
    }

    pub fn from_json(raw: &str) -> DdsResult<Self> {
        serde_json::from_str(raw)
            .map_err(|e| DdsError::Error(format!("invalid config JSON: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_config() {
        let json = r#"{
            "local":  { "domain_id": 0, "transport": "udp" },
            "remote": { "domain_id": 1, "transport": "tcp", "initial_peers": ["1.2.3.4:7400"] }
        }"#;
        let cfg = RouteGatewayConfig::from_json(json).unwrap();
        assert_eq!(cfg.local.domain_id, 0);
        assert_eq!(cfg.local.transport, "udp");
        assert_eq!(cfg.remote.domain_id, 1);
        assert_eq!(cfg.remote.initial_peers, vec!["1.2.3.4:7400"]);
        assert_eq!(cfg.auto_relay.filter, "*");
        assert_eq!(cfg.poll_period_ms, 50);
    }

    #[test]
    fn parses_full_config() {
        let json = r#"{
            "local":  { "domain_id": 0, "transport": "udp", "initial_peers": [] },
            "remote": { "domain_id": 1, "transport": "tcp", "initial_peers": ["10.0.0.1:7400"] },
            "auto_relay": { "filter": "sensor/*" },
            "poll_period_ms": 100
        }"#;
        let cfg = RouteGatewayConfig::from_json(json).unwrap();
        assert_eq!(cfg.auto_relay.filter, "sensor/*");
        assert_eq!(cfg.poll_period_ms, 100);
    }

    #[test]
    fn node_config_to_qos_carries_property() {
        let node = NodeConfig {
            domain_id: 1,
            transport: "tcp".to_string(),
            initial_peers: vec!["1.2.3.4:7400".to_string()],
            tls: None,
        };
        let qos = node.to_participant_qos();
        assert_eq!(qos.property.get("int2dds.transport"), Some("tcp"));
        assert_eq!(qos.property.get("int2dds.initial_peers"), Some("1.2.3.4:7400"));
        assert!(qos.property.get("int2dds.tls.ca_file").is_none());
    }

    #[test]
    fn parses_tls_config() {
        let json = r#"{
            "local":  { "domain_id": 0, "transport": "udp" },
            "remote": {
                "domain_id": 1,
                "transport": "tcp",
                "initial_peers": ["10.0.0.1:7650"],
                "tls": {
                    "ca_file":   "/etc/ssl/ca.pem",
                    "cert_file": "/etc/ssl/cert.pem",
                    "key_file":  "/etc/ssl/key.pem",
                    "server_name": "gateway.example.com",
                    "verify_peer": true
                }
            }
        }"#;
        let cfg = RouteGatewayConfig::from_json(json).unwrap();
        let tls = cfg.remote.tls.expect("tls present");
        assert_eq!(tls.ca_file, "/etc/ssl/ca.pem");
        assert_eq!(tls.server_name, "gateway.example.com");
        assert!(tls.verify_peer);
    }

    #[test]
    fn tls_config_mapped_to_qos_properties() {
        let node = NodeConfig {
            domain_id: 1,
            transport: "tcp".to_string(),
            initial_peers: vec![],
            tls: Some(TlsNodeConfig {
                ca_file: "/etc/ssl/ca.pem".to_string(),
                cert_file: "/etc/ssl/cert.pem".to_string(),
                key_file: "/etc/ssl/key.pem".to_string(),
                server_name: "gw.example.com".to_string(),
                verify_peer: true,
            }),
        };
        let qos = node.to_participant_qos();
        assert_eq!(qos.property.get("int2dds.tls.ca_file"), Some("/etc/ssl/ca.pem"));
        assert_eq!(qos.property.get("int2dds.tls.cert_file"), Some("/etc/ssl/cert.pem"));
        assert_eq!(qos.property.get("int2dds.tls.key_file"), Some("/etc/ssl/key.pem"));
        assert_eq!(qos.property.get("int2dds.tls.server_name"), Some("gw.example.com"));
        assert_eq!(qos.property.get("int2dds.tls.verify_peer"), Some("true"));
    }

    #[test]
    fn tls_server_name_defaults_to_localhost() {
        let json = r#"{
            "local":  { "domain_id": 0, "transport": "udp" },
            "remote": {
                "domain_id": 1, "transport": "tcp",
                "tls": { "ca_file": "/a", "cert_file": "/b", "key_file": "/c" }
            }
        }"#;
        let cfg = RouteGatewayConfig::from_json(json).unwrap();
        let tls = cfg.remote.tls.unwrap();
        assert_eq!(tls.server_name, "localhost");
        assert!(!tls.verify_peer);
    }
}
