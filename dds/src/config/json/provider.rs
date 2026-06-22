use std::{collections::HashMap, fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::{
    config::types::{
        entity_qos::{
            DataReaderQos as ConfigDataReaderQos, DataWriterQos as ConfigDataWriterQos,
            DomainParticipantQos as ConfigDomainParticipantQos, MergeQos,
            PublisherQos as ConfigPublisherQos, SubscriberQos as ConfigSubscriberQos,
            TopicQos as ConfigTopicQos,
        },
        qos_profile::{QosLibrary, QosProfile, SingleOrSeq},
    },
    core::error::{DdsError, DdsResult},
    dcps::{
        domain::qos::DomainParticipantQos,
        publication::qos::{DataWriterQos, PublisherQos},
        subscription::qos::{DataReaderQos, SubscriberQos},
        topic::qos::TopicQos,
    },
};

macro_rules! impl_get_qos {
    ($fn_name:ident, $internal_fn_name:ident, $config_qos_type:ty, $dcps_qos_type:ty, $field:ident, $doc:expr) => {
        #[doc = $doc]
        pub fn $fn_name(&self, path: &str) -> Option<$dcps_qos_type> {
            self.$internal_fn_name(path).map(|qos| qos.into())
        }

        pub(crate) fn $internal_fn_name(&self, path: &str) -> Option<$config_qos_type> {
            // Empty path: return first available QoS
            if path.is_empty() {
                for lib in self.libraries.values() {
                    // Try library-level QoS first
                    if let Some(qos_or_seq) = lib.$field.as_ref() {
                        match qos_or_seq {
                            SingleOrSeq::Single(qos) => return Some(qos.clone()),
                            SingleOrSeq::Seq(seq) => {
                                if let Some(first) = seq.first() {
                                    return Some(first.qos.clone());
                                }
                            }
                        }
                    }

                    // Try profile-level QoS
                    if let Some(profiles) = &lib.qos_profiles {
                        for profile in profiles {
                            if let Some(qos_or_seq) = profile.$field.as_ref() {
                                match qos_or_seq {
                                    SingleOrSeq::Single(qos) => return Some(qos.clone()),
                                    SingleOrSeq::Seq(seq) => {
                                        if let Some(first) = seq.first() {
                                            return Some(first.qos.clone());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                return None;
            }

            let parsed = QosPath::parse(path).ok()?;
            let library_name = &parsed.library;
            let profile_name = parsed.profile.as_deref();
            let qos_name = parsed.qos_name.as_deref();

            let qos_or_named = if let Some(profile_name) = profile_name {
                self.get_profile(library_name, profile_name)?.$field.as_ref()?
            } else {
                self.get_library(library_name)?.$field.as_ref()?
            };

            let qos = match qos_or_named {
                SingleOrSeq::Single(qos) => qos_name.is_none().then_some(qos),
                SingleOrSeq::Seq(seq) => {
                    if let Some(name) = qos_name {
                        seq.iter().find(|q| q.name == name).map(|q| &q.qos)
                    } else {
                        seq.first().map(|q| &q.qos)
                    }
                }
            }?
            .clone();

            if let Some(base_name) = &qos.base_name {
                // Get profile names from current library for base_name disambiguation
                let profile_names: Vec<&str> = self
                    .get_library(library_name)
                    .and_then(|lib| lib.qos_profiles.as_ref())
                    .map(|profiles| profiles.iter().map(|p| p.name.as_str()).collect())
                    .unwrap_or_default();
                let base_path =
                    QosPath::parse_base_name(base_name, library_name, profile_name, &profile_names)
                        .ok()?;
                let base_path_str = match (&base_path.profile, &base_path.qos_name) {
                    (Some(p), Some(q)) => format!("{}::{}::{}", base_path.library, p, q),
                    (Some(p), None) => format!("{}::{}", base_path.library, p),
                    (None, Some(q)) => format!("{}::{}", base_path.library, q),
                    (None, None) => base_path.library.clone(),
                };
                let base = self.$internal_fn_name(&base_path_str)?;
                return Some(qos.merge(&base));
            }

            Some(qos)
        }
    };
}

/// A provider for loading and retrieving QoS configurations from JSON files.
///
/// `QosProvider` allows loading QoS profiles from one or more JSON files and
/// retrieving QoS settings for various DDS entities. It supports:
/// - Loading multiple files with cross-file library references
/// - QoS inheritance via `base_name` attribute
/// - Both single QoS objects and named QoS sequences
///
/// # QoS Inheritance
///
/// QoS objects can inherit from other QoS objects using the `base_name` attribute.
/// The derived QoS inherits all policies from the base, and can override specific policies.
///
/// Supported `base_name` formats:
/// - `"QosName"` - inherits from a QoS with that name in the current profile
/// - `"ProfileName"` - inherits from the **first** QoS in the specified profile (same library)
/// - `"Profile::QosName"` - inherits from a named QoS in another profile (same library)
/// - `"Library::Profile::QosName"` - inherits from a QoS in another library
///
/// When a single-segment `base_name` matches a profile name in the current library,
/// it is interpreted as a profile reference (inheriting from the first QoS in that profile).
/// Otherwise, it is treated as a QoS name in the current profile.
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// use int2dds::config::json::QosProvider;
///
/// // Load from a single file
/// let provider = QosProvider::from_file(Path::new("qos_profiles.json"))?;
///
/// // Or load multiple files incrementally
/// let mut provider = QosProvider::new();
/// provider.load_file(Path::new("base_qos.json"))?;
/// provider.load_file(Path::new("app_qos.json"))?;
///
/// // Retrieve QoS settings using path syntax
/// let writer_qos = provider.get_datawriter_qos("MyLibrary::MyProfile::ReliableWriter");
/// # Ok::<(), int2dds::core::error::DdsError>(())
/// ```
#[derive(Default, Serialize, Deserialize)]
pub struct QosProvider {
    libraries: HashMap<String, QosLibrary>,
}

impl QosProvider {
    /// Creates a new empty `QosProvider`.
    ///
    /// Use [`load_file`](Self::load_file) to load QoS configurations.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new `QosProvider` and loads QoS configurations from the specified file.
    ///
    /// This is a convenience method equivalent to calling [`new`](Self::new) followed by
    /// [`load_file`](Self::load_file).
    ///
    /// # Arguments
    /// * `path` - Path to the JSON file containing QoS configurations
    ///
    /// # Errors
    /// Returns an error if the file cannot be read or parsed.
    pub fn from_file(path: &Path) -> DdsResult<Self> {
        let mut provider = Self::new();
        provider.load_file(path)?;
        Ok(provider)
    }

    /// Loads QoS configurations from a JSON or XML file.
    ///
    /// The format is chosen by extension (`.xml`/`.json`), falling back to sniffing
    /// the content. Multiple files can be loaded incrementally; a library with an
    /// existing name is replaced.
    ///
    /// # Arguments
    /// * `path` - Path to the JSON or XML file containing QoS configurations
    ///
    /// # Errors
    /// Returns an error if the file cannot be read or parsed.
    pub fn load_file(&mut self, path: &Path) -> DdsResult<()> {
        let content = fs::read_to_string(path)
            .map_err(|e| DdsError::Error(format!("Failed to read QoS file: {:?}", e)))?;
        if is_xml(path, &content) {
            self.load_xml(&content)
        } else {
            self.load_json(&content)
        }
    }

    pub(crate) fn load_xml(&mut self, xml: &str) -> DdsResult<()> {
        for lib in super::xml::parse_qos_libraries(xml)? {
            self.libraries.insert(lib.name.clone(), lib);
        }
        Ok(())
    }

    pub(crate) fn load_json(&mut self, json: &str) -> DdsResult<()> {
        // Try parsing as { "libraries": { ... } } format first
        if let Ok(parsed) = serde_json::from_str::<QosProvider>(json) {
            for (name, lib) in parsed.libraries {
                self.libraries.insert(name, lib);
            }
            return Ok(());
        }

        // Try parsing as single QosLibrary
        if let Ok(lib) = serde_json::from_str::<QosLibrary>(json) {
            self.libraries.insert(lib.name.clone(), lib);
            return Ok(());
        }

        Err(DdsError::Error(
            "Failed to parse QoS JSON: expected QosLibrary or { libraries: {...} }".to_string(),
        ))
    }

    /// Returns the path (`"Library::Profile"`) of the first profile marked with
    /// `is_default_profile = true` across all loaded libraries, or `None` if none is marked.
    ///
    pub fn default_profile_path(&self) -> Option<String> {
        for (lib_name, lib) in &self.libraries {
            if let Some(profiles) = &lib.qos_profiles {
                for profile in profiles {
                    if profile.is_default_profile == Some(true) {
                        return Some(format!("{}::{}", lib_name, profile.name));
                    }
                }
            }
        }
        None
    }

    pub(crate) fn get_library(&self, library_name: &str) -> Option<&QosLibrary> {
        self.libraries.get(library_name)
    }

    pub(crate) fn get_profile(
        &self,
        library_name: &str,
        profile_name: &str,
    ) -> Option<&QosProfile> {
        self.get_library(library_name)?
            .qos_profiles
            .as_ref()?
            .iter()
            .find(|profile| profile.name == profile_name)
    }

    impl_get_qos!(
        get_datawriter_qos,
        get_datawriter_qos_internal,
        ConfigDataWriterQos,
        DataWriterQos,
        datawriter_qos,
        "Retrieves a `DataWriterQos` from the provider.\n\n\
         # Arguments\n\
         * `path` - QoS path. Supported formats:\n\
           - `\"\"` (empty string) - returns the first available QoS\n\
           - `\"Library\"` - library-level QoS\n\
           - `\"Library::Profile\"` - profile-level QoS\n\
           - `\"Library::Profile::QosName\"` - named QoS in a sequence\n\n\
         # Returns\n\
         The resolved QoS with all `base_name` inheritance applied, or `None` if not found.\n\
         See [`QosProvider`] documentation for details on inheritance."
    );

    impl_get_qos!(
        get_datareader_qos,
        get_datareader_qos_internal,
        ConfigDataReaderQos,
        DataReaderQos,
        datareader_qos,
        "Retrieves a `DataReaderQos` from the provider. See `get_datawriter_qos` for argument details."
    );

    impl_get_qos!(
        get_topic_qos,
        get_topic_qos_internal,
        ConfigTopicQos,
        TopicQos,
        topic_qos,
        "Retrieves a `TopicQos` from the provider. See `get_datawriter_qos` for argument details."
    );

    impl_get_qos!(
        get_subscriber_qos,
        get_subscriber_qos_internal,
        ConfigSubscriberQos,
        SubscriberQos,
        subscriber_qos,
        "Retrieves a `SubscriberQos` from the provider. See `get_datawriter_qos` for argument details."
    );

    impl_get_qos!(
        get_publisher_qos,
        get_publisher_qos_internal,
        ConfigPublisherQos,
        PublisherQos,
        publisher_qos,
        "Retrieves a `PublisherQos` from the provider. See `get_datawriter_qos` for argument details."
    );

    impl_get_qos!(
        get_domainparticipant_qos,
        get_domainparticipant_qos_internal,
        ConfigDomainParticipantQos,
        DomainParticipantQos,
        domain_participant_qos,
        "Retrieves a `DomainParticipantQos` from the provider. See `get_datawriter_qos` for argument details."
    );
}

// Picks XML vs JSON by extension, falling back to the first non-space character.
fn is_xml(path: &Path, content: &str) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) if ext.eq_ignore_ascii_case("xml") => true,
        Some(ext) if ext.eq_ignore_ascii_case("json") => false,
        _ => content.trim_start().starts_with('<'),
    }
}

struct QosPath {
    library: String,
    profile: Option<String>,
    qos_name: Option<String>,
}

impl QosPath {
    /// Parses a QoS path string.
    ///
    /// Supported formats:
    /// - `Library` - library only
    /// - `Library::Profile` - library and profile
    /// - `Library::Profile::QosName` - library, profile, and QoS name
    fn parse(path: &str) -> DdsResult<Self> {
        let parts: Vec<&str> = path.split("::").collect();
        match parts.len() {
            1 => Ok(QosPath {
                library: parts[0].to_string(),
                profile: None,
                qos_name: None,
            }),
            2 => Ok(QosPath {
                library: parts[0].to_string(),
                profile: Some(parts[1].to_string()),
                qos_name: None,
            }),
            3 => Ok(QosPath {
                library: parts[0].to_string(),
                profile: Some(parts[1].to_string()),
                qos_name: Some(parts[2].to_string()),
            }),
            _ => Err(DdsError::Error(
                "Invalid QoS path format: expected 'Library', 'Library::Profile', or 'Library::Profile::QosName'".to_string(),
            )),
        }
    }

    /// Parses a base_name reference relative to current context.
    ///
    /// The `profile_names` parameter is used to disambiguate single-segment base_names:
    /// if the single segment matches a profile name, it's interpreted as a profile reference
    /// (inheriting from the first QoS in that profile). Otherwise, it's treated as a QoS name.
    fn parse_base_name(
        base_name: &str,
        current_library: &str,
        current_profile: Option<&str>,
        profile_names: &[&str],
    ) -> DdsResult<Self> {
        let parts: Vec<&str> = base_name.split("::").collect();
        match parts.len() {
            1 => {
                // Single segment: could be a QoS name OR a profile name
                // If it matches a profile name, treat as profile reference (first QoS in that profile)
                if profile_names.contains(&parts[0]) {
                    Ok(QosPath {
                        library: current_library.to_string(),
                        profile: Some(parts[0].to_string()),
                        qos_name: None, // Will resolve to first QoS in the profile
                    })
                } else {
                    // Treat as QoS name in current profile
                    Ok(QosPath {
                        library: current_library.to_string(),
                        profile: current_profile.map(|s| s.to_string()),
                        qos_name: Some(parts[0].to_string()),
                    })
                }
            }
            2 => Ok(QosPath {
                library: current_library.to_string(),
                profile: Some(parts[0].to_string()),
                qos_name: Some(parts[1].to_string()),
            }),
            3 => Ok(QosPath {
                library: parts[0].to_string(),
                profile: Some(parts[1].to_string()),
                qos_name: Some(parts[2].to_string()),
            }),
            _ => Err(DdsError::Error(
                "Invalid base_name format: expected 'QosName', 'ProfileName', 'Profile::QosName', or 'Library::Profile::QosName'".to_string(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl QosProvider {
        fn from_json(json: &str) -> DdsResult<Self> {
            let mut provider = Self::new();
            provider.load_json(json)?;
            Ok(provider)
        }
    }

    #[test]
    fn test_parse_library_with_single_datawriter_qos() {
        let json = r#"{
            "name": "TestLibrary",
            "datawriter_qos": {
                "reliability": {
                    "kind": "RELIABLE_RELIABILITY_QOS",
                    "max_blocking_time": { "sec": 1, "nanosec": 0 }
                }
            }
        }"#;

        let provider = QosProvider::from_json(json).unwrap();

        // verify library exists
        assert!(provider.get_library("TestLibrary").is_some());
        assert!(provider.get_library("NonExistent").is_none());

        // look up the single QoS
        let qos = provider.get_datawriter_qos_internal("TestLibrary");
        assert!(qos.is_some());
    }

    #[test]
    fn test_parse_library_with_seq_datawriter_qos() {
        let json = r#"{
            "name": "TestLibrary",
            "datawriter_qos": [
                {
                    "name": "ReliableQos",
                    "reliability": {
                        "kind": "RELIABLE_RELIABILITY_QOS",
                        "max_blocking_time": { "sec": 1, "nanosec": 0 }
                    }
                },
                {
                    "name": "BestEffortQos",
                    "reliability": {
                        "kind": "BEST_EFFORT_RELIABILITY_QOS",
                        "max_blocking_time": { "sec": 0, "nanosec": 0 }
                    }
                }
            ]
        }"#;

        let provider = QosProvider::from_json(json).unwrap();

        // when only the library is specified, return the first QoS in the seq
        let qos = provider.get_datawriter_qos_internal("TestLibrary");
        assert!(qos.is_some());
    }

    #[test]
    fn test_parse_library_with_profile() {
        let json = r#"{
            "name": "TestLibrary",
            "qos_profiles": [
                {
                    "name": "DefaultProfile",
                    "datawriter_qos": {
                        "durability": {
                            "kind": "TRANSIENT_LOCAL_DURABILITY_QOS"
                        }
                    }
                }
            ]
        }"#;

        let provider = QosProvider::from_json(json).unwrap();

        // verify profile exists
        assert!(provider.get_profile("TestLibrary", "DefaultProfile").is_some());
        assert!(provider.get_profile("TestLibrary", "NonExistent").is_none());

        // look up the single QoS within the profile
        let qos = provider.get_datawriter_qos_internal("TestLibrary::DefaultProfile");
        assert!(qos.is_some());
    }

    #[test]
    fn test_parse_library_with_profile_seq_qos() {
        let json = r#"{
            "name": "TestLibrary",
            "qos_profiles": [
                {
                    "name": "MyProfile",
                    "datareader_qos": [
                        {
                            "name": "Reader1",
                            "history": {
                                "kind": "KEEP_LAST_HISTORY_QOS",
                                "depth": 10
                            }
                        },
                        {
                            "name": "Reader2",
                            "history": {
                                "kind": "KEEP_ALL_HISTORY_QOS"
                            }
                        }
                    ]
                }
            ]
        }"#;

        let provider = QosProvider::from_json(json).unwrap();

        // look up a Seq QoS within the profile
        let qos = provider.get_datareader_qos_internal("TestLibrary::MyProfile::Reader1");
        assert!(qos.is_some());

        let qos = provider.get_datareader_qos_internal("TestLibrary::MyProfile::Reader2");
        assert!(qos.is_some());

        let qos = provider.get_datareader_qos_internal("TestLibrary::MyProfile::NonExistent");
        assert!(qos.is_none());
    }

    #[test]
    fn test_get_other_qos_types() {
        let json = r#"{
            "name": "TestLibrary",
            "topic_qos": {
                "durability": { "kind": "VOLATILE_DURABILITY_QOS" }
            },
            "publisher_qos": {
                "partition": { "name": { "element": ["partition1"] } }
            },
            "subscriber_qos": {
                "partition": { "name": { "element": ["partition2"] } }
            },
            "domain_participant_qos": {
                "user_data": { "value": "test" }
            }
        }"#;

        let provider = QosProvider::from_json(json).unwrap();

        assert!(provider.get_topic_qos_internal("TestLibrary").is_some());
        assert!(provider.get_publisher_qos_internal("TestLibrary").is_some());
        assert!(provider.get_subscriber_qos_internal("TestLibrary").is_some());
        assert!(provider.get_domainparticipant_qos_internal("TestLibrary").is_some());
    }

    #[test]
    fn test_invalid_json() {
        let json = r#"{ invalid json }"#;
        let result = QosProvider::from_json(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_library_name() {
        let json = r#"{
            "datawriter_qos": {}
        }"#;
        let result = QosProvider::from_json(json);
        assert!(result.is_err());
    }

    // ========== base_name inheritance tests ==========

    #[test]
    fn test_base_name_simple_inheritance() {
        let json = r#"{
            "name": "TestLibrary",
            "qos_profiles": [
                {
                    "name": "Profile1",
                    "datawriter_qos": [
                        {
                            "name": "BaseQos",
                            "reliability": {
                                "kind": "RELIABLE_RELIABILITY_QOS",
                                "max_blocking_time": { "sec": 5, "nanosec": 0 }
                            },
                            "durability": {
                                "kind": "TRANSIENT_LOCAL_DURABILITY_QOS"
                            }
                        },
                        {
                            "name": "DerivedQos",
                            "base_name": "BaseQos",
                            "history": {
                                "kind": "KEEP_LAST_HISTORY_QOS",
                                "depth": 100
                            }
                        }
                    ]
                }
            ]
        }"#;

        let provider = QosProvider::from_json(json).unwrap();

        let derived =
            provider.get_datawriter_qos_internal("TestLibrary::Profile1::DerivedQos").unwrap();

        // derived should have its own history
        assert!(derived.history.is_some());

        // derived should inherit reliability and durability from base
        assert!(derived.reliability.is_some());
        assert!(derived.durability.is_some());
    }

    #[test]
    fn test_base_name_override() {
        let json = r#"{
            "name": "TestLibrary",
            "qos_profiles": [
                {
                    "name": "Profile1",
                    "datawriter_qos": [
                        {
                            "name": "BaseQos",
                            "reliability": {
                                "kind": "BEST_EFFORT_RELIABILITY_QOS",
                                "max_blocking_time": { "sec": 0, "nanosec": 0 }
                            },
                            "history": {
                                "kind": "KEEP_LAST_HISTORY_QOS",
                                "depth": 1
                            }
                        },
                        {
                            "name": "DerivedQos",
                            "base_name": "BaseQos",
                            "reliability": {
                                "kind": "RELIABLE_RELIABILITY_QOS",
                                "max_blocking_time": { "sec": 10, "nanosec": 0 }
                            }
                        }
                    ]
                }
            ]
        }"#;

        let provider = QosProvider::from_json(json).unwrap();

        let derived =
            provider.get_datawriter_qos_internal("TestLibrary::Profile1::DerivedQos").unwrap();

        // derived should override reliability
        assert!(derived.reliability.is_some());

        // derived should inherit history from base
        assert!(derived.history.is_some());
    }

    #[test]
    fn test_base_name_chain_inheritance() {
        let json = r#"{
            "name": "TestLibrary",
            "qos_profiles": [
                {
                    "name": "Profile1",
                    "datawriter_qos": [
                        {
                            "name": "GrandparentQos",
                            "reliability": {
                                "kind": "RELIABLE_RELIABILITY_QOS",
                                "max_blocking_time": { "sec": 1, "nanosec": 0 }
                            }
                        },
                        {
                            "name": "ParentQos",
                            "base_name": "GrandparentQos",
                            "durability": {
                                "kind": "TRANSIENT_LOCAL_DURABILITY_QOS"
                            }
                        },
                        {
                            "name": "ChildQos",
                            "base_name": "ParentQos",
                            "history": {
                                "kind": "KEEP_ALL_HISTORY_QOS"
                            }
                        }
                    ]
                }
            ]
        }"#;

        let provider = QosProvider::from_json(json).unwrap();

        let child =
            provider.get_datawriter_qos_internal("TestLibrary::Profile1::ChildQos").unwrap();

        // child should have its own history
        assert!(child.history.is_some());

        // child should inherit durability from parent
        assert!(child.durability.is_some());

        // child should inherit reliability from grandparent
        assert!(child.reliability.is_some());
    }

    #[test]
    fn test_base_name_cross_profile() {
        let json = r#"{
            "name": "TestLibrary",
            "qos_profiles": [
                {
                    "name": "BaseProfile",
                    "datawriter_qos": [
                        {
                            "name": "SharedBase",
                            "reliability": {
                                "kind": "RELIABLE_RELIABILITY_QOS",
                                "max_blocking_time": { "sec": 3, "nanosec": 0 }
                            },
                            "durability": {
                                "kind": "VOLATILE_DURABILITY_QOS"
                            }
                        }
                    ]
                },
                {
                    "name": "DerivedProfile",
                    "datawriter_qos": [
                        {
                            "name": "DerivedQos",
                            "base_name": "BaseProfile::SharedBase",
                            "history": {
                                "kind": "KEEP_LAST_HISTORY_QOS",
                                "depth": 50
                            }
                        }
                    ]
                }
            ]
        }"#;

        let provider = QosProvider::from_json(json).unwrap();

        let derived = provider
            .get_datawriter_qos_internal("TestLibrary::DerivedProfile::DerivedQos")
            .unwrap();

        // derived should have its own history
        assert!(derived.history.is_some());

        // derived should inherit from BaseProfile::SharedBase
        assert!(derived.reliability.is_some());
        assert!(derived.durability.is_some());
    }

    #[test]
    fn test_base_name_cross_library() {
        let json = r#"{
            "libraries": {
                "BaseLibrary": {
                    "name": "BaseLibrary",
                    "qos_profiles": [
                        {
                            "name": "CommonProfile",
                            "datawriter_qos": [
                                {
                                    "name": "CommonBase",
                                    "reliability": {
                                        "kind": "RELIABLE_RELIABILITY_QOS",
                                        "max_blocking_time": { "sec": 2, "nanosec": 0 }
                                    }
                                }
                            ]
                        }
                    ]
                },
                "AppLibrary": {
                    "name": "AppLibrary",
                    "qos_profiles": [
                        {
                            "name": "AppProfile",
                            "datawriter_qos": [
                                {
                                    "name": "AppQos",
                                    "base_name": "BaseLibrary::CommonProfile::CommonBase",
                                    "durability": {
                                        "kind": "TRANSIENT_LOCAL_DURABILITY_QOS"
                                    }
                                }
                            ]
                        }
                    ]
                }
            }
        }"#;

        let provider = QosProvider::from_json(json).unwrap();

        let app_qos =
            provider.get_datawriter_qos_internal("AppLibrary::AppProfile::AppQos").unwrap();

        // app_qos should have its own durability
        assert!(app_qos.durability.is_some());

        // app_qos should inherit reliability from BaseLibrary::CommonProfile::CommonBase
        assert!(app_qos.reliability.is_some());
    }

    #[test]
    fn test_base_name_not_found() {
        let json = r#"{
            "name": "TestLibrary",
            "qos_profiles": [
                {
                    "name": "Profile1",
                    "datawriter_qos": [
                        {
                            "name": "DerivedQos",
                            "base_name": "NonExistentBase",
                            "history": {
                                "kind": "KEEP_ALL_HISTORY_QOS"
                            }
                        }
                    ]
                }
            ]
        }"#;

        let provider = QosProvider::from_json(json).unwrap();

        // Should return None because base_name doesn't exist
        let result = provider.get_datawriter_qos_internal("TestLibrary::Profile1::DerivedQos");
        assert!(result.is_none());
    }

    #[test]
    fn test_base_name_datareader_inheritance() {
        let json = r#"{
            "name": "TestLibrary",
            "qos_profiles": [
                {
                    "name": "Profile1",
                    "datareader_qos": [
                        {
                            "name": "BaseReader",
                            "reliability": {
                                "kind": "RELIABLE_RELIABILITY_QOS",
                                "max_blocking_time": { "sec": 1, "nanosec": 0 }
                            }
                        },
                        {
                            "name": "DerivedReader",
                            "base_name": "BaseReader",
                            "history": {
                                "kind": "KEEP_LAST_HISTORY_QOS",
                                "depth": 20
                            }
                        }
                    ]
                }
            ]
        }"#;

        let provider = QosProvider::from_json(json).unwrap();

        let derived =
            provider.get_datareader_qos_internal("TestLibrary::Profile1::DerivedReader").unwrap();

        assert!(derived.history.is_some());
        assert!(derived.reliability.is_some());
    }

    #[test]
    fn test_base_name_topic_inheritance() {
        let json = r#"{
            "name": "TestLibrary",
            "qos_profiles": [
                {
                    "name": "Profile1",
                    "topic_qos": [
                        {
                            "name": "BaseTopic",
                            "durability": {
                                "kind": "TRANSIENT_LOCAL_DURABILITY_QOS"
                            }
                        },
                        {
                            "name": "DerivedTopic",
                            "base_name": "BaseTopic",
                            "reliability": {
                                "kind": "RELIABLE_RELIABILITY_QOS",
                                "max_blocking_time": { "sec": 2, "nanosec": 0 }
                            }
                        }
                    ]
                }
            ]
        }"#;

        let provider = QosProvider::from_json(json).unwrap();

        let derived =
            provider.get_topic_qos_internal("TestLibrary::Profile1::DerivedTopic").unwrap();

        assert!(derived.reliability.is_some());
        assert!(derived.durability.is_some());
    }

    #[test]
    fn test_base_name_publisher_inheritance() {
        let json = r#"{
            "name": "TestLibrary",
            "qos_profiles": [
                {
                    "name": "Profile1",
                    "publisher_qos": [
                        {
                            "name": "BasePublisher",
                            "partition": {
                                "name": { "element": ["partition1", "partition2"] }
                            }
                        },
                        {
                            "name": "DerivedPublisher",
                            "base_name": "BasePublisher",
                            "group_data": { "value": "test" }
                        }
                    ]
                }
            ]
        }"#;

        let provider = QosProvider::from_json(json).unwrap();

        let derived =
            provider.get_publisher_qos_internal("TestLibrary::Profile1::DerivedPublisher").unwrap();

        assert!(derived.group_data.is_some());
        assert!(derived.partition.is_some());
    }

    #[test]
    fn test_base_name_subscriber_inheritance() {
        let json = r#"{
            "name": "TestLibrary",
            "qos_profiles": [
                {
                    "name": "Profile1",
                    "subscriber_qos": [
                        {
                            "name": "BaseSubscriber",
                            "partition": {
                                "name": { "element": ["sub_partition"] }
                            }
                        },
                        {
                            "name": "DerivedSubscriber",
                            "base_name": "BaseSubscriber",
                            "group_data": { "value": "subscriber_data" }
                        }
                    ]
                }
            ]
        }"#;

        let provider = QosProvider::from_json(json).unwrap();

        let derived = provider
            .get_subscriber_qos_internal("TestLibrary::Profile1::DerivedSubscriber")
            .unwrap();

        assert!(derived.group_data.is_some());
        assert!(derived.partition.is_some());
    }

    #[test]
    fn test_base_name_domain_participant_inheritance() {
        let json = r#"{
            "name": "TestLibrary",
            "qos_profiles": [
                {
                    "name": "Profile1",
                    "domain_participant_qos": [
                        {
                            "name": "BaseParticipant",
                            "user_data": { "value": "base_user_data" }
                        },
                        {
                            "name": "DerivedParticipant",
                            "base_name": "BaseParticipant",
                            "entity_factory": { "autoenable_created_entities": false }
                        }
                    ]
                }
            ]
        }"#;

        let provider = QosProvider::from_json(json).unwrap();

        let derived = provider
            .get_domainparticipant_qos_internal("TestLibrary::Profile1::DerivedParticipant")
            .unwrap();

        assert!(derived.entity_factory.is_some());
        assert!(derived.user_data.is_some());
    }

    #[test]
    fn test_no_base_name() {
        let json = r#"{
            "name": "TestLibrary",
            "qos_profiles": [
                {
                    "name": "Profile1",
                    "datawriter_qos": {
                        "reliability": {
                            "kind": "RELIABLE_RELIABILITY_QOS",
                            "max_blocking_time": { "sec": 1, "nanosec": 0 }
                        }
                    }
                }
            ]
        }"#;

        let provider = QosProvider::from_json(json).unwrap();

        let qos = provider.get_datawriter_qos_internal("TestLibrary::Profile1").unwrap();

        // works correctly even without base_name
        assert!(qos.reliability.is_some());
        assert!(qos.base_name.is_none());
    }

    #[test]
    fn test_base_name_multiple_policies_merge() {
        let json = r#"{
            "name": "TestLibrary",
            "qos_profiles": [
                {
                    "name": "Profile1",
                    "datawriter_qos": [
                        {
                            "name": "FullBase",
                            "reliability": {
                                "kind": "RELIABLE_RELIABILITY_QOS",
                                "max_blocking_time": { "sec": 5, "nanosec": 0 }
                            },
                            "durability": {
                                "kind": "TRANSIENT_LOCAL_DURABILITY_QOS"
                            },
                            "history": {
                                "kind": "KEEP_LAST_HISTORY_QOS",
                                "depth": 10
                            },
                            "deadline": {
                                "period": { "sec": 1, "nanosec": 0 }
                            }
                        },
                        {
                            "name": "PartialOverride",
                            "base_name": "FullBase",
                            "reliability": {
                                "kind": "BEST_EFFORT_RELIABILITY_QOS",
                                "max_blocking_time": { "sec": 0, "nanosec": 0 }
                            },
                            "history": {
                                "kind": "KEEP_ALL_HISTORY_QOS"
                            }
                        }
                    ]
                }
            ]
        }"#;

        let provider = QosProvider::from_json(json).unwrap();

        let derived =
            provider.get_datawriter_qos_internal("TestLibrary::Profile1::PartialOverride").unwrap();

        // overridden
        assert!(derived.reliability.is_some());
        assert!(derived.history.is_some());

        // inherited
        assert!(derived.durability.is_some());
        assert!(derived.deadline.is_some());
    }

    #[test]
    fn test_base_name_profile_only_inheritance() {
        // base_name="Profile1" inherits from first QoS in Profile1
        let json = r#"{
            "name": "TestLibrary",
            "qos_profiles": [
                {
                    "name": "Profile1",
                    "datawriter_qos": [
                        {
                            "name": "FirstWriterQos",
                            "reliability": {
                                "kind": "RELIABLE_RELIABILITY_QOS",
                                "max_blocking_time": { "sec": 5, "nanosec": 0 }
                            },
                            "durability": {
                                "kind": "TRANSIENT_LOCAL_DURABILITY_QOS"
                            }
                        },
                        {
                            "name": "SecondWriterQos",
                            "reliability": {
                                "kind": "BEST_EFFORT_RELIABILITY_QOS",
                                "max_blocking_time": { "sec": 0, "nanosec": 0 }
                            }
                        }
                    ]
                },
                {
                    "name": "Profile2",
                    "datawriter_qos": [
                        {
                            "name": "DerivedWriterQos",
                            "base_name": "Profile1",
                            "history": {
                                "kind": "KEEP_ALL_HISTORY_QOS"
                            }
                        }
                    ]
                }
            ]
        }"#;

        let provider = QosProvider::from_json(json).unwrap();

        // Profile2::DerivedWriterQos should inherit from Profile1's first QoS (FirstWriterQos)
        let derived = provider
            .get_datawriter_qos_internal("TestLibrary::Profile2::DerivedWriterQos")
            .unwrap();

        // own setting
        assert!(derived.history.is_some());

        // inherited from Profile1's first QoS (FirstWriterQos)
        // Reliability should be RELIABLE (from FirstWriterQos) with max_blocking_time of 5 sec
        let reliability = derived.reliability.as_ref().unwrap();
        assert_eq!(reliability.max_blocking_time.as_ref().unwrap().sec, 5);
        // Durability should be inherited from FirstWriterQos
        assert!(derived.durability.is_some());
    }

    #[test]
    fn test_qos_path_parse_single_name() {
        // No profile names provided, so "BaseQos" is treated as QoS name
        let path = QosPath::parse_base_name("BaseQos", "MyLib", Some("MyProfile"), &[]).unwrap();
        assert_eq!(path.library.as_str(), "MyLib");
        assert_eq!(path.profile.as_deref(), Some("MyProfile"));
        assert_eq!(path.qos_name.as_deref(), Some("BaseQos"));
    }

    #[test]
    fn test_qos_path_parse_single_name_as_profile() {
        // "BaseProfile" matches a profile name, so it's treated as profile reference
        let path = QosPath::parse_base_name(
            "BaseProfile",
            "MyLib",
            Some("MyProfile"),
            &["BaseProfile", "OtherProfile"],
        )
        .unwrap();
        assert_eq!(path.library.as_str(), "MyLib");
        assert_eq!(path.profile.as_deref(), Some("BaseProfile"));
        assert_eq!(path.qos_name, None); // Will resolve to first QoS in BaseProfile
    }

    #[test]
    fn test_qos_path_parse_profile_and_name() {
        let path =
            QosPath::parse_base_name("OtherProfile::BaseQos", "MyLib", Some("MyProfile"), &[])
                .unwrap();
        assert_eq!(path.library.as_str(), "MyLib");
        assert_eq!(path.profile.as_deref(), Some("OtherProfile"));
        assert_eq!(path.qos_name.as_deref(), Some("BaseQos"));
    }

    #[test]
    fn test_qos_path_parse_full_path() {
        let path = QosPath::parse_base_name(
            "OtherLib::OtherProfile::BaseQos",
            "MyLib",
            Some("MyProfile"),
            &[],
        )
        .unwrap();
        assert_eq!(path.library.as_str(), "OtherLib");
        assert_eq!(path.profile.as_deref(), Some("OtherProfile"));
        assert_eq!(path.qos_name.as_deref(), Some("BaseQos"));
    }

    #[test]
    fn test_qos_path_parse_invalid() {
        let result = QosPath::parse_base_name("A::B::C::D", "MyLib", Some("MyProfile"), &[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_qos_path_parse_no_current_profile() {
        let path = QosPath::parse_base_name("BaseQos", "MyLib", None, &[]).unwrap();
        assert_eq!(path.library.as_str(), "MyLib");
        assert_eq!(path.profile, None);
        assert_eq!(path.qos_name.as_deref(), Some("BaseQos"));
    }

    // ========== multi-file loading tests ==========

    #[test]
    fn test_load_multiple_files() {
        let mut provider = QosProvider::new();

        // Load first file with BaseLibrary
        let json1 = r#"{
            "name": "BaseLibrary",
            "qos_profiles": [
                {
                    "name": "BaseProfile",
                    "datawriter_qos": [
                        {
                            "name": "BaseQos",
                            "reliability": {
                                "kind": "RELIABLE_RELIABILITY_QOS",
                                "max_blocking_time": { "sec": 1, "nanosec": 0 }
                            }
                        }
                    ]
                }
            ]
        }"#;
        provider.load_json(json1).unwrap();

        // Load second file with AppLibrary that references BaseLibrary
        let json2 = r#"{
            "name": "AppLibrary",
            "qos_profiles": [
                {
                    "name": "AppProfile",
                    "datawriter_qos": [
                        {
                            "name": "AppQos",
                            "base_name": "BaseLibrary::BaseProfile::BaseQos",
                            "durability": {
                                "kind": "TRANSIENT_LOCAL_DURABILITY_QOS"
                            }
                        }
                    ]
                }
            ]
        }"#;
        provider.load_json(json2).unwrap();

        // Both libraries should be accessible
        assert!(provider.get_library("BaseLibrary").is_some());
        assert!(provider.get_library("AppLibrary").is_some());

        // Cross-library inheritance should work
        let app_qos =
            provider.get_datawriter_qos_internal("AppLibrary::AppProfile::AppQos").unwrap();

        assert!(app_qos.durability.is_some());
        assert!(app_qos.reliability.is_some());
    }

    #[test]
    fn test_load_multiple_libraries_in_one_json() {
        let json = r#"{
            "libraries": {
                "Lib1": {
                    "name": "Lib1",
                    "datawriter_qos": {
                        "reliability": {
                            "kind": "RELIABLE_RELIABILITY_QOS",
                            "max_blocking_time": { "sec": 1, "nanosec": 0 }
                        }
                    }
                },
                "Lib2": {
                    "name": "Lib2",
                    "datawriter_qos": {
                        "durability": {
                            "kind": "TRANSIENT_LOCAL_DURABILITY_QOS"
                        }
                    }
                }
            }
        }"#;

        let provider = QosProvider::from_json(json).unwrap();

        assert!(provider.get_library("Lib1").is_some());
        assert!(provider.get_library("Lib2").is_some());

        let qos1 = provider.get_datawriter_qos_internal("Lib1").unwrap();
        assert!(qos1.reliability.is_some());

        let qos2 = provider.get_datawriter_qos_internal("Lib2").unwrap();
        assert!(qos2.durability.is_some());
    }

    #[test]
    fn test_load_overwrites_existing_library() {
        let mut provider = QosProvider::new();

        let json1 = r#"{
            "name": "MyLibrary",
            "datawriter_qos": {
                "reliability": {
                    "kind": "BEST_EFFORT_RELIABILITY_QOS",
                    "max_blocking_time": { "sec": 0, "nanosec": 0 }
                }
            }
        }"#;
        provider.load_json(json1).unwrap();

        let json2 = r#"{
            "name": "MyLibrary",
            "datawriter_qos": {
                "durability": {
                    "kind": "TRANSIENT_LOCAL_DURABILITY_QOS"
                }
            }
        }"#;
        provider.load_json(json2).unwrap();

        // Second load overwrites first
        let qos = provider.get_datawriter_qos_internal("MyLibrary").unwrap();
        assert!(qos.durability.is_some());
        assert!(qos.reliability.is_none());
    }

    #[test]
    fn test_empty_loader() {
        let provider = QosProvider::new();
        assert!(provider.get_library("Any").is_none());
        assert!(provider.get_datawriter_qos_internal("Any").is_none());
    }

    #[test]
    fn test_empty_path_returns_first_qos() {
        let json = r#"{
            "name": "TestLibrary",
            "datawriter_qos": {
                "reliability": {
                    "kind": "RELIABLE_RELIABILITY_QOS",
                    "max_blocking_time": { "sec": 1, "nanosec": 0 }
                }
            }
        }"#;

        let provider = QosProvider::from_json(json).unwrap();

        // Empty path returns first available QoS
        let qos = provider.get_datawriter_qos_internal("");
        assert!(qos.is_some());
        assert!(qos.unwrap().reliability.is_some());
    }

    #[test]
    fn test_empty_path_with_no_qos() {
        let provider = QosProvider::new();

        // Empty path on empty provider returns None
        let qos = provider.get_datawriter_qos_internal("");
        assert!(qos.is_none());
    }

    #[test]
    fn test_empty_path_returns_first_from_profile() {
        let json = r#"{
            "name": "TestLibrary",
            "qos_profiles": [
                {
                    "name": "MyProfile",
                    "datawriter_qos": {
                        "durability": {
                            "kind": "TRANSIENT_LOCAL_DURABILITY_QOS"
                        }
                    }
                }
            ]
        }"#;

        let provider = QosProvider::from_json(json).unwrap();

        // Empty path returns first QoS from profile when no library-level QoS exists
        let qos = provider.get_datawriter_qos_internal("");
        assert!(qos.is_some());
        assert!(qos.unwrap().durability.is_some());
    }
}
