//! Resolved transport-layer configuration derived from `PropertyQosPolicy`.
//!
//! `TransportConfig` is intentionally a small POD struct so socket/sender layers
//! can carry a single `Copy` value instead of importing the full DCPS QoS module
//! (preserves the `rtps::transport` → `dcps` one-way dependency).
//!
//! Transport-tunable property keys (e.g. [`PROP_MULTICAST_TTL`]) live in the
//! `dcps::infrastructure::qos_policy` module so they form the public API surface
//! alongside the property setter helpers; this file just consumes them.

use crate::dcps::infrastructure::qos_policy::{PropertyQosPolicy, PROP_MULTICAST_TTL};

/// IPv4 multicast TTL fallback. Matches RFC 1112 / `IP_MULTICAST_TTL` defaults
/// on Linux and Windows (`1` — link-local only).
pub(crate) const DEFAULT_MULTICAST_TTL: u8 = 1;

#[derive(Debug, Clone, Copy)]
pub(crate) struct TransportConfig {
    pub multicast_ttl: u8,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self { multicast_ttl: DEFAULT_MULTICAST_TTL }
    }
}

impl TransportConfig {
    /// Resolve transport-layer parameters from a `PropertyQosPolicy`.
    ///
    /// Takes `&PropertyQosPolicy` (not `&DomainParticipantQos`) so the transport
    /// layer never imports `dcps::domain` — the only DCPS dependency stays the
    /// single property container type.
    ///
    /// Unknown keys are ignored. Parse failures (e.g. `"abc"`, `"256"`, `"-1"`)
    /// log a warning and fall back to the default — a per-key typo must never
    /// abort `DomainParticipant` creation.
    pub(crate) fn from_property(property: &PropertyQosPolicy) -> Self {
        let multicast_ttl = property
            .find_property(PROP_MULTICAST_TTL)
            .and_then(|v| match v.parse::<u8>() {
                Ok(ttl) => Some(ttl),
                Err(e) => {
                    log::warn!(
                        "[TransportConfig] invalid {} property value '{}': {}. \
                         Falling back to default {}.",
                        PROP_MULTICAST_TTL,
                        v,
                        e,
                        DEFAULT_MULTICAST_TTL
                    );
                    None
                }
            })
            .unwrap_or(DEFAULT_MULTICAST_TTL);

        Self { multicast_ttl }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_when_property_missing() {
        assert_eq!(TransportConfig::default().multicast_ttl, DEFAULT_MULTICAST_TTL);
        assert_eq!(
            TransportConfig::from_property(&PropertyQosPolicy::default()).multicast_ttl,
            DEFAULT_MULTICAST_TTL
        );
    }

    #[test]
    fn parses_valid_u8_values_including_boundaries() {
        for raw in ["0", "1", "32", "255"] {
            let mut p = PropertyQosPolicy::default();
            p.add_property(PROP_MULTICAST_TTL, raw, false);
            assert_eq!(
                TransportConfig::from_property(&p).multicast_ttl,
                raw.parse::<u8>().unwrap(),
                "parse {raw}"
            );
        }
    }

    #[test]
    fn invalid_values_fall_back_to_default() {
        for bad in ["abc", "256", "-1", ""] {
            let mut p = PropertyQosPolicy::default();
            p.add_property(PROP_MULTICAST_TTL, bad, false);
            assert_eq!(
                TransportConfig::from_property(&p).multicast_ttl,
                DEFAULT_MULTICAST_TTL,
                "{bad} should fall back"
            );
        }
    }
}
