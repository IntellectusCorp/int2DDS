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
    core::error::DdsResult,
    domain::domain_participant::DomainParticipant,
    subscription::{
        data_reader::DataReader,
        sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
    },
};

use super::topic_relay::TopicRelay;

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
    BUILTIN_TOPIC_NAMES.iter().any(|n| *n == name)
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

/// Per-direction discovery state.
struct DiscoverySide {
    publication_reader: DataReader<PublicationBuiltinTopicData>,
    seen_topics: HashSet<String>,
    seen_instances: HashSet<InstanceHandle>,
}

impl DiscoverySide {
    fn new(participant: &DomainParticipant) -> DdsResult<Self> {
        let builtin = participant.get_builtin_subscriber()?;
        let publication_reader = builtin
            .lookup_datareader::<PublicationBuiltinTopicData>("DCPSPublication")?;
        Ok(Self {
            publication_reader,
            seen_topics: HashSet::new(),
            seen_instances: HashSet::new(),
        })
    }
}

/// Automatically discovers and relays topics between LocalNode and RemoteNode.
pub struct AutoRelay {
    local: Arc<DomainParticipant>,
    remote: Arc<DomainParticipant>,
    filter: TopicFilter,
    local_side: Mutex<DiscoverySide>,
    remote_side: Mutex<DiscoverySide>,
    relays: Mutex<HashMap<String, Arc<TopicRelay>>>,
}

impl AutoRelay {
    /// Build a new AutoRelay over the given participant pair.
    /// `filter` controls which topic names are relayed (default: "*").
    pub fn new(
        local: Arc<DomainParticipant>,
        remote: Arc<DomainParticipant>,
        filter: TopicFilter,
    ) -> DdsResult<Self> {
        let local_side = DiscoverySide::new(&local)?;
        let remote_side = DiscoverySide::new(&remote)?;
        Ok(Self {
            local,
            remote,
            filter,
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
            collect_new_publications(&mut side, &self.filter, &mut new_topics);
        }
        {
            let mut side = self.remote_side.lock().unwrap();
            collect_new_publications(&mut side, &self.filter, &mut new_topics);
        }

        let mut created = 0;
        let mut relays = self.relays.lock().unwrap();
        for (topic_name, type_object) in new_topics {
            if relays.contains_key(&topic_name) {
                continue;
            }
            match TopicRelay::new(&self.local, &self.remote, &topic_name, type_object) {
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
        let relays: Vec<Arc<TopicRelay>> =
            self.relays.lock().unwrap().values().cloned().collect();
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
        if !side.seen_instances.insert(info.instance_handle) {
            continue;
        }

        let pub_data = match sample.data() {
            Ok(d) => d,
            Err(_) => continue,
        };

        let topic_name = pub_data.topic_name().to_string();
        if is_builtin_topic(&topic_name) {
            continue;
        }
        if !filter.matches(&topic_name) {
            continue;
        }
        if !side.seen_topics.insert(topic_name.clone()) {
            continue;
        }

        match pub_data.type_object() {
            Some(type_obj) => out.push((topic_name, type_obj.clone())),
            None => {
                log::warn!(
                    "[AutoRelay] publication on '{}' has no TypeObject; skipping",
                    topic_name
                );
            }
        }
    }
}
