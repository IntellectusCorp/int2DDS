//! AutoRelay: Automatically discovers topics and creates TopicRelays.
//!
//! AutoRelay polls the builtin DCPSPublication readers on both LocalNode and
//! RemoteNode. When a new non-builtin publication is discovered whose topic
//! name matches the user-supplied filter, AutoRelay extracts its TypeObject
//! and creates a [`TopicRelay`] for that topic.
//!
//! Once registered, calling [`AutoRelay::forward_once`] runs forwarding for
//! every active relay (both directions).

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use crate::{
    common::{
        builtin::topic::publication_builtin_topic_data::PublicationBuiltinTopicData,
        instance_handle::InstanceHandle,
    },
    config::json::QosProvider,
    core::error::{DdsError, DdsResult},
    domain::domain_participant::DomainParticipant,
    publication::qos::DataWriterQos,
    subscription::{
        data_reader::DataReader,
        qos::DataReaderQos,
        sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
    },
    xtypes::{SharedTypeRegistry, TypeResolver},
};

use super::{
    config::{AutoRelayConfig, TopicRelayRule},
    topic_relay::{default_relay_reader_qos, default_relay_writer_qos, TopicRelay, TopicRelayQos},
};

/// Builtin topics created by every DomainParticipant.
/// Never relayed across the gateway.
const BUILTIN_TOPIC_NAMES: &[&str] = &[
    "DCPSParticipant",
    "DCPSPublication",
    "DCPSSubscription",
    "DCPSParticipantMessage",
    "DCPSTopic",
];

fn is_builtin_topic(name: &str) -> bool {
    BUILTIN_TOPIC_NAMES.contains(&name)
}

/// Simple glob-style topic filter. Supports "*" as match-all and a single
/// trailing "*" as prefix match (e.g. "sensor/*").
#[derive(Debug, Clone)]
pub struct TopicFilter {
    pattern: String,
}

impl TopicFilter {
    pub fn new(pattern: impl Into<String>) -> Self {
        Self { pattern: pattern.into() }
    }

    pub fn matches(&self, topic_name: &str) -> bool {
        if self.pattern == "*" {
            return true;
        }
        if let Some(prefix) = self.pattern.strip_suffix('*') {
            return topic_name.starts_with(prefix);
        }
        topic_name == self.pattern
    }
}

impl Default for TopicFilter {
    fn default() -> Self {
        Self::new("*")
    }
}

/// Resolves the four QoS objects of a [`TopicRelay`] for a given topic name.
///
/// Precedence (per endpoint, evaluated independently):
///   1. The first [`TopicRelayRule`] whose pattern matches the topic and that
///      specifies this endpoint's QoS profile.
///   2. The corresponding `default_*_qos` from [`AutoRelayConfig`].
///   3. The built-in fallback ([`default_relay_reader_qos`] /
///      [`default_relay_writer_qos`]): `KEEP_ALL` history plus type defaults.
///
/// Rules are stored in declaration order so users can put most-specific
/// patterns first and a catch-all last.
pub struct QosResolver {
    provider: QosProvider,
    default_local_reader: Option<String>,
    default_local_writer: Option<String>,
    default_remote_reader: Option<String>,
    default_remote_writer: Option<String>,
    rules: Vec<CompiledRule>,
}

struct CompiledRule {
    pattern: TopicFilter,
    local_reader: Option<String>,
    local_writer: Option<String>,
    remote_reader: Option<String>,
    remote_writer: Option<String>,
}

impl QosResolver {
    /// Builds a resolver that always returns the built-in fallback QoS.
    /// Equivalent to the pre-customization behavior.
    pub fn empty() -> Self {
        Self {
            provider: QosProvider::new(),
            default_local_reader: None,
            default_local_writer: None,
            default_remote_reader: None,
            default_remote_writer: None,
            rules: Vec::new(),
        }
    }

    /// Build a resolver from an [`AutoRelayConfig`] and the list of
    /// [`TopicRelayRule`] entries. Any referenced profile path is validated
    /// eagerly; an unresolvable reference returns [`DdsError::Error`].
    pub fn from_config(auto_relay: &AutoRelayConfig, rules: &[TopicRelayRule]) -> DdsResult<Self> {
        let provider = auto_relay.build_qos_provider()?;

        let check_reader = |path: &Option<String>, where_: &str| -> DdsResult<()> {
            if let Some(p) = path {
                if provider.get_datareader_qos(p).is_none() {
                    return Err(DdsError::Error(format!(
                        "DataReader QoS profile '{}' not found ({})",
                        p, where_
                    )));
                }
            }
            Ok(())
        };
        let check_writer = |path: &Option<String>, where_: &str| -> DdsResult<()> {
            if let Some(p) = path {
                if provider.get_datawriter_qos(p).is_none() {
                    return Err(DdsError::Error(format!(
                        "DataWriter QoS profile '{}' not found ({})",
                        p, where_
                    )));
                }
            }
            Ok(())
        };

        check_reader(&auto_relay.default_local_reader_qos, "default_local_reader_qos")?;
        check_writer(&auto_relay.default_local_writer_qos, "default_local_writer_qos")?;
        check_reader(&auto_relay.default_remote_reader_qos, "default_remote_reader_qos")?;
        check_writer(&auto_relay.default_remote_writer_qos, "default_remote_writer_qos")?;

        let mut compiled = Vec::with_capacity(rules.len());
        for (i, rule) in rules.iter().enumerate() {
            let ctx = format!("topic_relays[{}] '{}'", i, rule.topic_pattern);
            check_reader(&rule.local_reader_qos, &ctx)?;
            check_writer(&rule.local_writer_qos, &ctx)?;
            check_reader(&rule.remote_reader_qos, &ctx)?;
            check_writer(&rule.remote_writer_qos, &ctx)?;
            compiled.push(CompiledRule {
                pattern: TopicFilter::new(rule.topic_pattern.clone()),
                local_reader: rule.local_reader_qos.clone(),
                local_writer: rule.local_writer_qos.clone(),
                remote_reader: rule.remote_reader_qos.clone(),
                remote_writer: rule.remote_writer_qos.clone(),
            });
        }

        Ok(Self {
            provider,
            default_local_reader: auto_relay.default_local_reader_qos.clone(),
            default_local_writer: auto_relay.default_local_writer_qos.clone(),
            default_remote_reader: auto_relay.default_remote_reader_qos.clone(),
            default_remote_writer: auto_relay.default_remote_writer_qos.clone(),
            rules: compiled,
        })
    }

    fn resolve_reader(
        &self,
        topic: &str,
        pick: fn(&CompiledRule) -> &Option<String>,
        default: &Option<String>,
    ) -> DataReaderQos {
        for rule in &self.rules {
            if rule.pattern.matches(topic) {
                if let Some(path) = pick(rule) {
                    return self
                        .provider
                        .get_datareader_qos(path)
                        .unwrap_or_else(default_relay_reader_qos);
                }
                break;
            }
        }
        if let Some(path) = default {
            return self.provider.get_datareader_qos(path).unwrap_or_else(default_relay_reader_qos);
        }
        default_relay_reader_qos()
    }

    fn resolve_writer(
        &self,
        topic: &str,
        pick: fn(&CompiledRule) -> &Option<String>,
        default: &Option<String>,
    ) -> DataWriterQos {
        for rule in &self.rules {
            if rule.pattern.matches(topic) {
                if let Some(path) = pick(rule) {
                    return self
                        .provider
                        .get_datawriter_qos(path)
                        .unwrap_or_else(default_relay_writer_qos);
                }
                break;
            }
        }
        if let Some(path) = default {
            return self.provider.get_datawriter_qos(path).unwrap_or_else(default_relay_writer_qos);
        }
        default_relay_writer_qos()
    }

    /// Returns the resolved [`TopicRelayQos`] for `topic_name`.
    pub fn resolve(&self, topic_name: &str) -> TopicRelayQos {
        TopicRelayQos {
            local_reader: self.resolve_reader(
                topic_name,
                |r| &r.local_reader,
                &self.default_local_reader,
            ),
            local_writer: self.resolve_writer(
                topic_name,
                |r| &r.local_writer,
                &self.default_local_writer,
            ),
            remote_reader: self.resolve_reader(
                topic_name,
                |r| &r.remote_reader,
                &self.default_remote_reader,
            ),
            remote_writer: self.resolve_writer(
                topic_name,
                |r| &r.remote_writer,
                &self.default_remote_writer,
            ),
        }
    }
}

impl Default for QosResolver {
    fn default() -> Self {
        Self::empty()
    }
}

/// Per-direction discovery state.
struct DiscoverySide {
    publication_reader: DataReader<PublicationBuiltinTopicData>,
    seen_topics: HashSet<String>,
    seen_instances: HashSet<InstanceHandle>,
}

impl DiscoverySide {
    fn new(participant: &DomainParticipant) -> DdsResult<Self> {
        let builtin = participant.get_builtin_subscriber()?;
        let publication_reader =
            builtin.lookup_datareader::<PublicationBuiltinTopicData>("DCPSPublication")?;
        Ok(Self { publication_reader, seen_topics: HashSet::new(), seen_instances: HashSet::new() })
    }
}

/// Automatically discovers and relays topics between LocalNode and RemoteNode.
pub struct AutoRelay {
    local: Arc<DomainParticipant>,
    remote: Arc<DomainParticipant>,
    filter: TopicFilter,
    qos_resolver: Arc<QosResolver>,
    local_side: Mutex<DiscoverySide>,
    remote_side: Mutex<DiscoverySide>,
    relays: Mutex<HashMap<String, Arc<TopicRelay>>>,
}

impl AutoRelay {
    /// Build a new AutoRelay over the given participant pair.
    ///
    /// `filter` controls which topic names are relayed (default: "*"). Every
    /// discovered topic uses the built-in fallback QoS
    /// ([`TopicRelayQos::default`]). Use [`AutoRelay::with_qos`] to supply
    /// per-topic QoS from a [`QosResolver`].
    pub fn new(
        local: Arc<DomainParticipant>,
        remote: Arc<DomainParticipant>,
        filter: TopicFilter,
    ) -> DdsResult<Self> {
        Self::with_qos(local, remote, filter, Arc::new(QosResolver::empty()))
    }

    /// Build an AutoRelay that consults `qos_resolver` for each discovered
    /// topic before creating its [`TopicRelay`].
    pub fn with_qos(
        local: Arc<DomainParticipant>,
        remote: Arc<DomainParticipant>,
        filter: TopicFilter,
        qos_resolver: Arc<QosResolver>,
    ) -> DdsResult<Self> {
        let local_side = DiscoverySide::new(&local)?;
        let remote_side = DiscoverySide::new(&remote)?;
        Ok(Self {
            local,
            remote,
            filter,
            qos_resolver,
            local_side: Mutex::new(local_side),
            remote_side: Mutex::new(remote_side),
            relays: Mutex::new(HashMap::new()),
        })
    }

    /// Topic names currently being relayed.
    pub fn active_topics(&self) -> Vec<String> {
        self.relays.lock().unwrap().keys().cloned().collect()
    }

    /// Number of active relays.
    pub fn relay_count(&self) -> usize {
        self.relays.lock().unwrap().len()
    }

    /// Poll both sides for new publications. For each newly discovered topic
    /// matching the filter, create a TopicRelay. Returns the number of new
    /// relays created.
    pub fn discover_once(&self) -> DdsResult<usize> {
        let mut new_topics: Vec<(String, crate::xtypes::TypeObject)> = Vec::new();

        // Collect newly discovered topics from both sides.
        {
            let mut side = self.local_side.lock().unwrap();
            let registry = self.local.get_rtps_participant()?.type_registry();
            collect_new_publications(&mut side, &self.filter, &registry, &mut new_topics);
        }
        {
            let mut side = self.remote_side.lock().unwrap();
            let registry = self.remote.get_rtps_participant()?.type_registry();
            collect_new_publications(&mut side, &self.filter, &registry, &mut new_topics);
        }

        let mut created = 0;
        let mut relays = self.relays.lock().unwrap();
        for (topic_name, type_object) in new_topics {
            if relays.contains_key(&topic_name) {
                continue;
            }
            let qos = self.qos_resolver.resolve(&topic_name);
            match TopicRelay::new_with_qos(&self.local, &self.remote, &topic_name, type_object, qos)
            {
                Ok(relay) => {
                    relays.insert(topic_name, Arc::new(relay));
                    created += 1;
                }
                Err(e) => {
                    log::warn!(
                        "[AutoRelay] failed to create TopicRelay for '{}': {:?}",
                        topic_name,
                        e
                    );
                }
            }
        }
        Ok(created)
    }

    /// Run one bidirectional forwarding pass over every active relay.
    /// Returns total forwarded sample count.
    pub fn forward_once(&self) -> DdsResult<usize> {
        let relays: Vec<Arc<TopicRelay>> = self.relays.lock().unwrap().values().cloned().collect();
        let mut total = 0;
        for relay in relays {
            let (l2r, r2l) = relay.forward_once()?;
            total += l2r + r2l;
        }
        Ok(total)
    }
}

fn collect_new_publications(
    side: &mut DiscoverySide,
    filter: &TopicFilter,
    registry: &SharedTypeRegistry,
    out: &mut Vec<(String, crate::xtypes::TypeObject)>,
) {
    let samples = match side.publication_reader.read(
        i32::MAX,
        &[SampleStateKind::ANY_SAMPLE_STATE],
        &[ViewStateKind::ANY_VIEW_STATE],
        &[InstanceStateKind::ALIVE_INSTANCE_STATE],
    ) {
        Ok(s) => s,
        Err(crate::core::error::DdsError::NoData) => return,
        Err(e) => {
            log::warn!("[AutoRelay] failed to read DCPSPublication: {:?}", e);
            return;
        }
    };

    for sample in samples.iter() {
        let info = sample.sample_info();
        if !info.valid_data {
            continue;
        }
        if side.seen_instances.contains(&info.instance_handle) {
            continue;
        }

        let pub_data = match sample.data() {
            Ok(d) => d,
            Err(_) => continue,
        };

        let topic_name = pub_data.topic_name().to_string();
        if is_builtin_topic(&topic_name) || !filter.matches(&topic_name) {
            side.seen_instances.insert(info.instance_handle);
            continue;
        }
        if side.seen_topics.contains(&topic_name) {
            side.seen_instances.insert(info.instance_handle);
            continue;
        }

        // TypeObject acquisition: inline 0x0072 (legacy peers) first, else
        // resolve the discovered TypeIdentifier against the participant's
        // TypeRegistry (populated on demand via TypeLookup).
        let type_object = if let Some(type_obj) = pub_data.type_object() {
            Some(type_obj.clone())
        } else if let Some(type_id) = pub_data.type_identifier() {
            registry.read().ok().and_then(|r| r.resolve_complete(type_id))
        } else {
            log::warn!(
                "[AutoRelay] publication on '{}' has no TypeObject or TypeIdentifier; skipping",
                topic_name
            );
            side.seen_instances.insert(info.instance_handle);
            continue;
        };

        match type_object {
            Some(type_obj) => {
                side.seen_instances.insert(info.instance_handle);
                side.seen_topics.insert(topic_name.clone());
                out.push((topic_name, type_obj));
            }
            None => {
                log::debug!("[AutoRelay] type for '{}' not yet resolved; will retry", topic_name);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::qos_policy::{HistoryQosPolicyKind, ReliabilityQosPolicyKind};

    fn cfg_with_inline(profiles: serde_json::Value) -> AutoRelayConfig {
        AutoRelayConfig { qos_profiles: Some(profiles), ..Default::default() }
    }

    #[test]
    fn empty_resolver_returns_builtin_defaults() {
        let resolver = QosResolver::empty();
        let qos = resolver.resolve("any/topic");
        // Built-in fallback: KEEP_ALL history on all four endpoints.
        assert!(matches!(qos.local_reader.history.kind, HistoryQosPolicyKind::KeepAll));
        assert!(matches!(qos.remote_writer.history.kind, HistoryQosPolicyKind::KeepAll));
    }

    #[test]
    fn default_qos_applied_when_no_rules_match() {
        let profiles = serde_json::json!({
            "name": "L",
            "qos_profiles": [{
                "name": "R",
                "datareader_qos": { "reliability": { "kind": "RELIABLE_RELIABILITY_QOS" } },
                "datawriter_qos": { "reliability": { "kind": "RELIABLE_RELIABILITY_QOS" } }
            }]
        });
        let mut cfg = cfg_with_inline(profiles);
        cfg.default_local_reader_qos = Some("L::R".to_string());
        cfg.default_remote_writer_qos = Some("L::R".to_string());

        let resolver = QosResolver::from_config(&cfg, &[]).unwrap();
        let qos = resolver.resolve("x/y");
        assert!(matches!(qos.local_reader.reliability.kind, ReliabilityQosPolicyKind::Reliable));
        assert!(matches!(qos.remote_writer.reliability.kind, ReliabilityQosPolicyKind::Reliable));
        // Unspecified defaults fall back to built-in.
        assert!(matches!(qos.remote_reader.history.kind, HistoryQosPolicyKind::KeepAll));
    }

    #[test]
    fn rule_overrides_default_on_glob_match() {
        let profiles = serde_json::json!({
            "name": "L",
            "qos_profiles": [
                {
                    "name": "Def",
                    "datareader_qos": { "reliability": { "kind": "BEST_EFFORT_RELIABILITY_QOS" } }
                },
                {
                    "name": "Rel",
                    "datareader_qos": { "reliability": { "kind": "RELIABLE_RELIABILITY_QOS" } }
                }
            ]
        });
        let mut cfg = cfg_with_inline(profiles);
        cfg.default_local_reader_qos = Some("L::Def".to_string());

        let rules = vec![TopicRelayRule {
            topic_pattern: "sensor/*".to_string(),
            local_reader_qos: Some("L::Rel".to_string()),
            ..Default::default()
        }];
        let resolver = QosResolver::from_config(&cfg, &rules).unwrap();

        // Matching topic → rule wins.
        let matched = resolver.resolve("sensor/temp");
        assert!(matches!(
            matched.local_reader.reliability.kind,
            ReliabilityQosPolicyKind::Reliable
        ));
        // Non-matching topic → default wins.
        let other = resolver.resolve("other/topic");
        assert!(matches!(
            other.local_reader.reliability.kind,
            ReliabilityQosPolicyKind::BestEffort
        ));
    }

    #[test]
    fn first_matching_rule_wins_even_if_field_unset() {
        // Two overlapping rules. First matches but leaves local_writer unset:
        // the default (or built-in) should apply — we must NOT scan the
        // second rule for the missing field.
        let profiles = serde_json::json!({
            "name": "L",
            "qos_profiles": [{
                "name": "Rel",
                "datawriter_qos": { "reliability": { "kind": "RELIABLE_RELIABILITY_QOS" } }
            }]
        });
        let cfg = cfg_with_inline(profiles);
        let rules = vec![
            TopicRelayRule {
                topic_pattern: "a/*".to_string(),
                // No local_writer_qos set.
                ..Default::default()
            },
            TopicRelayRule {
                topic_pattern: "*".to_string(),
                local_writer_qos: Some("L::Rel".to_string()),
                ..Default::default()
            },
        ];
        let resolver = QosResolver::from_config(&cfg, &rules).unwrap();
        let qos = resolver.resolve("a/foo");
        // Writer falls to built-in (KEEP_ALL + default Reliable reliability),
        // NOT the second rule's reliable override.
        assert!(matches!(qos.local_writer.history.kind, HistoryQosPolicyKind::KeepAll));
    }

    #[test]
    fn unresolvable_profile_path_is_rejected_eagerly() {
        let cfg = AutoRelayConfig {
            default_local_reader_qos: Some("Missing::Path".to_string()),
            ..Default::default()
        };
        match QosResolver::from_config(&cfg, &[]) {
            Err(DdsError::Error(msg)) => assert!(msg.contains("Missing::Path")),
            Ok(_) => panic!("expected error for missing profile"),
            Err(other) => panic!("expected DdsError::Error, got {:?}", other),
        }
    }

    #[test]
    fn example_office_gw_with_qos_resolves_end_to_end() {
        // Guards the shipped example: any schema drift in parsing or
        // resolution will fail here rather than at deployment time.
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/tcp/wan_2network/office_gw_with_qos.json");
        if !path.exists() {
            // Example lives in the sibling examples repo; skip if absent.
            return;
        }
        let cfg =
            super::super::RouteGatewayConfig::from_file(&path).expect("example config parses");
        let resolver = QosResolver::from_config(&cfg.auto_relay, &cfg.topic_relays)
            .expect("example config resolves");

        // control/* rule → fully RELIABLE on all four endpoints.
        let control = resolver.resolve("control/motor");
        assert!(matches!(
            control.local_reader.reliability.kind,
            ReliabilityQosPolicyKind::Reliable
        ));
        assert!(matches!(
            control.remote_writer.reliability.kind,
            ReliabilityQosPolicyKind::Reliable
        ));

        // Any other topic → defaults apply: LAN reliable, WAN best-effort.
        let telemetry = resolver.resolve("telemetry/cpu");
        assert!(matches!(
            telemetry.local_reader.reliability.kind,
            ReliabilityQosPolicyKind::Reliable
        ));
        assert!(matches!(
            telemetry.remote_writer.reliability.kind,
            ReliabilityQosPolicyKind::BestEffort
        ));
    }

    #[test]
    fn topic_filter_glob_patterns() {
        assert!(TopicFilter::new("*").matches("anything"));
        assert!(TopicFilter::new("sensor/*").matches("sensor/temp"));
        assert!(!TopicFilter::new("sensor/*").matches("actuator/x"));
        assert!(TopicFilter::new("exact").matches("exact"));
        assert!(!TopicFilter::new("exact").matches("exact2"));
    }
}
