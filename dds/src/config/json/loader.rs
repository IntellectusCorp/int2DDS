use std::{collections::HashMap, fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::{
    config::types::{
        entity_qos::{
            DataReaderQos, DataWriterQos, DomainParticipantQos, MergeQos, PublisherQos,
            SubscriberQos, TopicQos,
        },
        qos_profile::{QosLibrary, QosProfile, SingleOrSeq},
    },
    core::error::{DdsError, DdsResult},
};

macro_rules! impl_get_qos {
    ($fn_name:ident, $qos_type:ty, $field:ident) => {
        pub(crate) fn $fn_name(
            &self,
            library_name: &str,
            profile_name: Option<&str>,
            qos_name: Option<&str>,
        ) -> Option<$qos_type> {
            let qos_or_named = if let Some(profile_name) = profile_name {
                self.get_profile(library_name, profile_name)?.$field.as_ref()?
            } else {
                self.get_library(library_name)?.$field.as_ref()?
            };

            let qos = match qos_or_named {
                SingleOrSeq::Single(qos) => qos_name.is_none().then_some(qos),
                SingleOrSeq::Seq(seq) => {
                    let name = qos_name?;
                    seq.iter().find(|q| q.name == name).map(|q| &q.qos)
                }
            }?
            .clone();

            if let Some(base_name) = &qos.base_name {
                let path = QosPath::parse_base_name(base_name, library_name, profile_name).ok()?;
                let base = self.$fn_name(
                    path.library.as_deref()?,
                    path.profile.as_deref(),
                    Some(&path.qos_name),
                )?;
                return Some(qos.merge(&base));
            }

            Some(qos)
        }
    };
}

#[derive(Default, Serialize, Deserialize)]
pub(crate) struct QosLoader {
    libraries: HashMap<String, QosLibrary>,
}

#[allow(dead_code)]
impl QosLoader {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn from_file(path: &Path) -> DdsResult<Self> {
        let mut loader = Self::new();
        loader.load_file(path)?;
        Ok(loader)
    }

    pub(crate) fn from_json(json: &str) -> DdsResult<Self> {
        let mut loader = Self::new();
        loader.load_json(json)?;
        Ok(loader)
    }

    pub(crate) fn load_file(&mut self, path: &Path) -> DdsResult<()> {
        let content = fs::read_to_string(path)
            .map_err(|e| DdsError::Error(format!("Failed to read QoS file: {:?}", e)))?;
        self.load_json(&content)
    }

    pub(crate) fn load_json(&mut self, json: &str) -> DdsResult<()> {
        // Try parsing as { "libraries": { ... } } format first
        if let Ok(parsed) = serde_json::from_str::<QosLoader>(json) {
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

    impl_get_qos!(get_datawriter_qos, DataWriterQos, datawriter_qos);
    impl_get_qos!(get_datareader_qos, DataReaderQos, datareader_qos);
    impl_get_qos!(get_topic_qos, TopicQos, topic_qos);
    impl_get_qos!(get_subscriber_qos, SubscriberQos, subscriber_qos);
    impl_get_qos!(get_publisher_qos, PublisherQos, publisher_qos);
    impl_get_qos!(get_domainparticipant_qos, DomainParticipantQos, domain_participant_qos);
}

struct QosPath {
    library: Option<String>,
    profile: Option<String>,
    qos_name: String,
}

impl QosPath {
    fn parse_base_name(
        base_name: &str,
        current_library: &str,
        current_profile: Option<&str>,
    ) -> DdsResult<Self> {
        let parts: Vec<&str> = base_name.split("::").collect();
        match parts.len() {
            1 => Ok(QosPath {
                library: Some(current_library.to_string()),
                profile: current_profile.map(|s| s.to_string()),
                qos_name: parts[0].to_string(),
            }),
            2 => Ok(QosPath {
                library: Some(current_library.to_string()),
                profile: Some(parts[0].to_string()),
                qos_name: parts[1].to_string(),
            }),
            3 => Ok(QosPath {
                library: Some(parts[0].to_string()),
                profile: Some(parts[1].to_string()),
                qos_name: parts[2].to_string(),
            }),
            _ => Err(DdsError::Error("Invalid base_name format: expected 'QosName', 'Profile::QosName', or 'Library::Profile::QosName'".to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let loader = QosLoader::from_json(json).unwrap();

        // library 존재 확인
        assert!(loader.get_library("TestLibrary").is_some());
        assert!(loader.get_library("NonExistent").is_none());

        // 단일 QoS 조회 - qos_name: None
        let qos = loader.get_datawriter_qos("TestLibrary", None, None);
        assert!(qos.is_some());

        // 단일 QoS인데 qos_name 지정하면 None
        let qos = loader.get_datawriter_qos("TestLibrary", None, Some("SomeName"));
        assert!(qos.is_none());
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

        let loader = QosLoader::from_json(json).unwrap();

        // Seq에서 이름으로 조회
        let qos = loader.get_datawriter_qos("TestLibrary", None, Some("ReliableQos"));
        assert!(qos.is_some());

        let qos = loader.get_datawriter_qos("TestLibrary", None, Some("BestEffortQos"));
        assert!(qos.is_some());

        // 없는 이름
        let qos = loader.get_datawriter_qos("TestLibrary", None, Some("NonExistent"));
        assert!(qos.is_none());

        // Seq인데 qos_name: None이면 None
        let qos = loader.get_datawriter_qos("TestLibrary", None, None);
        assert!(qos.is_none());
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

        let loader = QosLoader::from_json(json).unwrap();

        // profile 존재 확인
        assert!(loader.get_profile("TestLibrary", "DefaultProfile").is_some());
        assert!(loader.get_profile("TestLibrary", "NonExistent").is_none());

        // profile 내 단일 QoS 조회
        let qos = loader.get_datawriter_qos("TestLibrary", Some("DefaultProfile"), None);
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

        let loader = QosLoader::from_json(json).unwrap();

        // profile 내 Seq QoS 조회
        let qos = loader.get_datareader_qos("TestLibrary", Some("MyProfile"), Some("Reader1"));
        assert!(qos.is_some());

        let qos = loader.get_datareader_qos("TestLibrary", Some("MyProfile"), Some("Reader2"));
        assert!(qos.is_some());

        let qos = loader.get_datareader_qos("TestLibrary", Some("MyProfile"), Some("NonExistent"));
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

        let loader = QosLoader::from_json(json).unwrap();

        assert!(loader.get_topic_qos("TestLibrary", None, None).is_some());
        assert!(loader.get_publisher_qos("TestLibrary", None, None).is_some());
        assert!(loader.get_subscriber_qos("TestLibrary", None, None).is_some());
        assert!(loader.get_domainparticipant_qos("TestLibrary", None, None).is_some());
    }

    #[test]
    fn test_invalid_json() {
        let json = r#"{ invalid json }"#;
        let result = QosLoader::from_json(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_library_name() {
        let json = r#"{
            "datawriter_qos": {}
        }"#;
        let result = QosLoader::from_json(json);
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

        let loader = QosLoader::from_json(json).unwrap();

        let derived =
            loader.get_datawriter_qos("TestLibrary", Some("Profile1"), Some("DerivedQos")).unwrap();

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

        let loader = QosLoader::from_json(json).unwrap();

        let derived =
            loader.get_datawriter_qos("TestLibrary", Some("Profile1"), Some("DerivedQos")).unwrap();

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

        let loader = QosLoader::from_json(json).unwrap();

        let child =
            loader.get_datawriter_qos("TestLibrary", Some("Profile1"), Some("ChildQos")).unwrap();

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

        let loader = QosLoader::from_json(json).unwrap();

        let derived = loader
            .get_datawriter_qos("TestLibrary", Some("DerivedProfile"), Some("DerivedQos"))
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

        let loader = QosLoader::from_json(json).unwrap();

        let app_qos =
            loader.get_datawriter_qos("AppLibrary", Some("AppProfile"), Some("AppQos")).unwrap();

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

        let loader = QosLoader::from_json(json).unwrap();

        // Should return None because base_name doesn't exist
        let result = loader.get_datawriter_qos("TestLibrary", Some("Profile1"), Some("DerivedQos"));
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

        let loader = QosLoader::from_json(json).unwrap();

        let derived = loader
            .get_datareader_qos("TestLibrary", Some("Profile1"), Some("DerivedReader"))
            .unwrap();

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

        let loader = QosLoader::from_json(json).unwrap();

        let derived =
            loader.get_topic_qos("TestLibrary", Some("Profile1"), Some("DerivedTopic")).unwrap();

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

        let loader = QosLoader::from_json(json).unwrap();

        let derived = loader
            .get_publisher_qos("TestLibrary", Some("Profile1"), Some("DerivedPublisher"))
            .unwrap();

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

        let loader = QosLoader::from_json(json).unwrap();

        let derived = loader
            .get_subscriber_qos("TestLibrary", Some("Profile1"), Some("DerivedSubscriber"))
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

        let loader = QosLoader::from_json(json).unwrap();

        let derived = loader
            .get_domainparticipant_qos("TestLibrary", Some("Profile1"), Some("DerivedParticipant"))
            .unwrap();

        assert!(derived.entity_factory.is_some());
        assert!(derived.user_data.is_some());
    }

    #[test]
    fn test_base_name_library_level_qos() {
        let json = r#"{
            "name": "TestLibrary",
            "datawriter_qos": [
                {
                    "name": "LibraryBase",
                    "reliability": {
                        "kind": "RELIABLE_RELIABILITY_QOS",
                        "max_blocking_time": { "sec": 1, "nanosec": 0 }
                    }
                },
                {
                    "name": "LibraryDerived",
                    "base_name": "LibraryBase",
                    "durability": {
                        "kind": "TRANSIENT_LOCAL_DURABILITY_QOS"
                    }
                }
            ]
        }"#;

        let loader = QosLoader::from_json(json).unwrap();

        let derived =
            loader.get_datawriter_qos("TestLibrary", None, Some("LibraryDerived")).unwrap();

        assert!(derived.durability.is_some());
        assert!(derived.reliability.is_some());
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

        let loader = QosLoader::from_json(json).unwrap();

        let qos = loader.get_datawriter_qos("TestLibrary", Some("Profile1"), None).unwrap();

        // base_name 없이도 정상 동작
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

        let loader = QosLoader::from_json(json).unwrap();

        let derived = loader
            .get_datawriter_qos("TestLibrary", Some("Profile1"), Some("PartialOverride"))
            .unwrap();

        // overridden
        assert!(derived.reliability.is_some());
        assert!(derived.history.is_some());

        // inherited
        assert!(derived.durability.is_some());
        assert!(derived.deadline.is_some());
    }

    #[test]
    fn test_qos_path_parse_single_name() {
        let path = QosPath::parse_base_name("BaseQos", "MyLib", Some("MyProfile")).unwrap();
        assert_eq!(path.library.as_deref(), Some("MyLib"));
        assert_eq!(path.profile.as_deref(), Some("MyProfile"));
        assert_eq!(path.qos_name, "BaseQos");
    }

    #[test]
    fn test_qos_path_parse_profile_and_name() {
        let path =
            QosPath::parse_base_name("OtherProfile::BaseQos", "MyLib", Some("MyProfile")).unwrap();
        assert_eq!(path.library.as_deref(), Some("MyLib"));
        assert_eq!(path.profile.as_deref(), Some("OtherProfile"));
        assert_eq!(path.qos_name, "BaseQos");
    }

    #[test]
    fn test_qos_path_parse_full_path() {
        let path =
            QosPath::parse_base_name("OtherLib::OtherProfile::BaseQos", "MyLib", Some("MyProfile"))
                .unwrap();
        assert_eq!(path.library.as_deref(), Some("OtherLib"));
        assert_eq!(path.profile.as_deref(), Some("OtherProfile"));
        assert_eq!(path.qos_name, "BaseQos");
    }

    #[test]
    fn test_qos_path_parse_invalid() {
        let result = QosPath::parse_base_name("A::B::C::D", "MyLib", Some("MyProfile"));
        assert!(result.is_err());
    }

    #[test]
    fn test_qos_path_parse_no_current_profile() {
        let path = QosPath::parse_base_name("BaseQos", "MyLib", None).unwrap();
        assert_eq!(path.library.as_deref(), Some("MyLib"));
        assert_eq!(path.profile, None);
        assert_eq!(path.qos_name, "BaseQos");
    }

    // ========== multi-file loading tests ==========

    #[test]
    fn test_load_multiple_files() {
        let mut loader = QosLoader::new();

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
        loader.load_json(json1).unwrap();

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
        loader.load_json(json2).unwrap();

        // Both libraries should be accessible
        assert!(loader.get_library("BaseLibrary").is_some());
        assert!(loader.get_library("AppLibrary").is_some());

        // Cross-library inheritance should work
        let app_qos = loader
            .get_datawriter_qos("AppLibrary", Some("AppProfile"), Some("AppQos"))
            .unwrap();

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

        let loader = QosLoader::from_json(json).unwrap();

        assert!(loader.get_library("Lib1").is_some());
        assert!(loader.get_library("Lib2").is_some());

        let qos1 = loader.get_datawriter_qos("Lib1", None, None).unwrap();
        assert!(qos1.reliability.is_some());

        let qos2 = loader.get_datawriter_qos("Lib2", None, None).unwrap();
        assert!(qos2.durability.is_some());
    }

    #[test]
    fn test_load_overwrites_existing_library() {
        let mut loader = QosLoader::new();

        let json1 = r#"{
            "name": "MyLibrary",
            "datawriter_qos": {
                "reliability": {
                    "kind": "BEST_EFFORT_RELIABILITY_QOS",
                    "max_blocking_time": { "sec": 0, "nanosec": 0 }
                }
            }
        }"#;
        loader.load_json(json1).unwrap();

        let json2 = r#"{
            "name": "MyLibrary",
            "datawriter_qos": {
                "durability": {
                    "kind": "TRANSIENT_LOCAL_DURABILITY_QOS"
                }
            }
        }"#;
        loader.load_json(json2).unwrap();

        // Second load overwrites first
        let qos = loader.get_datawriter_qos("MyLibrary", None, None).unwrap();
        assert!(qos.durability.is_some());
        assert!(qos.reliability.is_none());
    }

    #[test]
    fn test_empty_loader() {
        let loader = QosLoader::new();
        assert!(loader.get_library("Any").is_none());
        assert!(loader.get_datawriter_qos("Any", None, None).is_none());
    }
}
