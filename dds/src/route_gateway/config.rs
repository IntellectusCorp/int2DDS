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
}

impl NodeConfig {
    pub fn to_participant_qos(&self) -> DomainParticipantQos {
        let mut property = PropertyQosPolicy::default();
        property.set("int2dds.transport", &self.transport);
        if !self.initial_peers.is_empty() {
            property.set("int2dds.initial_peers", &self.initial_peers.join(","));
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
        };
        let qos = node.to_participant_qos();
        assert_eq!(qos.property.get("int2dds.transport"), Some("tcp"));
        assert_eq!(qos.property.get("int2dds.initial_peers"), Some("1.2.3.4:7400"));
    }
}
