//! Configuration loading for the Route Gateway binary.
//!
//! Loads a JSON file describing the LocalNode (LAN side) and RemoteNode
//! (WAN side) participants and the topic filter applied by AutoRelay.

use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::{
    config::json::QosProvider,
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

/// AutoRelay configuration.
///
/// `filter` restricts which topic names are relayed (default: "*").
///
/// The remaining fields configure QoS for the DataReader/DataWriter pairs
/// that each [`super::topic_relay::TopicRelay`] creates on the LAN and WAN
/// participants. Without these, the gateway falls back to library defaults
/// (History = KEEP_ALL on both sides, all other policies are
/// `Default::default()`). Mismatched policies (e.g. a local publisher using
/// RELIABLE + TRANSIENT_LOCAL while the gateway reader stays on the defaults)
/// cause SEDP to reject the match, silently dropping samples. These fields
/// let operators align the relay's endpoints with the local pub/sub.
///
/// QoS values are profile path strings (e.g. `"Lib::Profile"` or
/// `"Lib::Profile::QosName"`) resolved through [`QosProvider`]. Profiles can
/// be supplied inline via `qos_profiles` or loaded from an external file via
/// `qos_profiles_file`; both may be used together.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoRelayConfig {
    #[serde(default = "default_filter")]
    pub filter: String,

    /// Path to an external `qos_profiles.json` file (same format as
    /// [`QosProvider`] accepts). Loaded before `qos_profiles`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qos_profiles_file: Option<String>,

    /// Inline QoS profile definitions. Accepts the same shape as a JSON file
    /// passed to [`QosProvider::load_json`] — either a single `QosLibrary`
    /// object or a `{ "libraries": { ... } }` wrapper.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qos_profiles: Option<serde_json::Value>,

    /// Default QoS profile path applied to the LAN-side DataReader of every
    /// auto-discovered relay. Overridden by matching `topic_relays` rules.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_local_reader_qos: Option<String>,

    /// Default QoS profile path applied to the LAN-side DataWriter of every
    /// auto-discovered relay.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_local_writer_qos: Option<String>,

    /// Default QoS profile path applied to the WAN-side DataReader of every
    /// auto-discovered relay.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_remote_reader_qos: Option<String>,

    /// Default QoS profile path applied to the WAN-side DataWriter of every
    /// auto-discovered relay.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_remote_writer_qos: Option<String>,
}

fn default_filter() -> String {
    "*".to_string()
}

impl Default for AutoRelayConfig {
    fn default() -> Self {
        Self {
            filter: default_filter(),
            qos_profiles_file: None,
            qos_profiles: None,
            default_local_reader_qos: None,
            default_local_writer_qos: None,
            default_remote_reader_qos: None,
            default_remote_writer_qos: None,
        }
    }
}

impl AutoRelayConfig {
    /// Build a [`QosProvider`] from `qos_profiles_file` and `qos_profiles`.
    /// External file is loaded first, then the inline block is merged on top
    /// (inline wins on library-name collisions).
    pub fn build_qos_provider(&self) -> DdsResult<QosProvider> {
        let mut provider = QosProvider::new();
        if let Some(path) = &self.qos_profiles_file {
            provider.load_file(Path::new(path))?;
        }
        if let Some(value) = &self.qos_profiles {
            let raw = serde_json::to_string(value).map_err(|e| {
                DdsError::Error(format!("failed to re-serialize qos_profiles: {e}"))
            })?;
            provider.load_json(&raw)?;
        }
        Ok(provider)
    }
}

/// Per-topic QoS override rule.
///
/// The first rule whose `topic_pattern` matches a newly discovered topic is
/// applied. Each of the four QoS fields is optional: when unset, the matching
/// `default_*_qos` from [`AutoRelayConfig`] is used, and when that is also
/// unset, the built-in fallback applies. Patterns follow the same glob syntax
/// as [`super::auto_relay::TopicFilter`]: `"*"` matches all, a single trailing
/// `*` is a prefix match (e.g. `"sensor/*"`), anything else is exact.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TopicRelayRule {
    pub topic_pattern: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_reader_qos: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_writer_qos: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_reader_qos: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_writer_qos: Option<String>,
}

/// Top-level Route Gateway configuration file format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteGatewayConfig {
    pub local: NodeConfig,
    pub remote: NodeConfig,
    #[serde(default)]
    pub auto_relay: AutoRelayConfig,
    /// Per-topic QoS override rules. Evaluated in order; the first match wins.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub topic_relays: Vec<TopicRelayRule>,
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
    fn parses_qos_override_fields() {
        let json = r#"{
            "local":  { "domain_id": 0, "transport": "udp" },
            "remote": { "domain_id": 1, "transport": "tcp", "initial_peers": ["1.2.3.4:7400"] },
            "auto_relay": {
                "filter": "*",
                "qos_profiles": {
                    "name": "GwLib",
                    "qos_profiles": [
                        {
                            "name": "Reliable",
                            "datareader_qos": { "reliability": { "kind": "RELIABLE_RELIABILITY_QOS" } },
                            "datawriter_qos": { "reliability": { "kind": "RELIABLE_RELIABILITY_QOS" } }
                        }
                    ]
                },
                "default_local_reader_qos":  "GwLib::Reliable",
                "default_local_writer_qos":  "GwLib::Reliable",
                "default_remote_reader_qos": "GwLib::Reliable",
                "default_remote_writer_qos": "GwLib::Reliable"
            },
            "topic_relays": [
                {
                    "topic_pattern": "sensor/*",
                    "local_reader_qos":  "GwLib::Reliable",
                    "remote_writer_qos": "GwLib::Reliable"
                }
            ]
        }"#;
        let cfg = RouteGatewayConfig::from_json(json).unwrap();
        assert_eq!(cfg.auto_relay.default_local_reader_qos.as_deref(), Some("GwLib::Reliable"));
        assert_eq!(cfg.topic_relays.len(), 1);
        assert_eq!(cfg.topic_relays[0].topic_pattern, "sensor/*");
        assert!(cfg.topic_relays[0].local_writer_qos.is_none());
    }

    #[test]
    fn qos_provider_resolves_inline_profile() {
        let json = r#"{
            "local":  { "domain_id": 0, "transport": "udp" },
            "remote": { "domain_id": 1, "transport": "tcp" },
            "auto_relay": {
                "qos_profiles": {
                    "name": "GwLib",
                    "qos_profiles": [{
                        "name": "KeepAll",
                        "datareader_qos": { "history": { "kind": "KEEP_ALL_HISTORY_QOS" } }
                    }]
                }
            }
        }"#;
        let cfg = RouteGatewayConfig::from_json(json).unwrap();
        let provider = cfg.auto_relay.build_qos_provider().unwrap();
        let qos = provider
            .get_datareader_qos("GwLib::KeepAll")
            .expect("profile resolves");
        matches!(qos.history.kind, crate::infrastructure::qos_policy::HistoryQosPolicyKind::KeepAll);
    }

    #[test]
    fn defaults_empty_when_not_specified() {
        let json = r#"{
            "local":  { "domain_id": 0, "transport": "udp" },
            "remote": { "domain_id": 1, "transport": "tcp" }
        }"#;
        let cfg = RouteGatewayConfig::from_json(json).unwrap();
        assert!(cfg.auto_relay.default_local_reader_qos.is_none());
        assert!(cfg.auto_relay.qos_profiles.is_none());
        assert!(cfg.topic_relays.is_empty());
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
