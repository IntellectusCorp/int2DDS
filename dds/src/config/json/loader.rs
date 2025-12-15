use std::{collections::HashMap, fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::{
    config::types::{
        entity_qos::{
            DataReaderQos, DataWriterQos, DomainParticipantQos, PublisherQos, SubscriberQos,
            TopicQos,
        },
        qos_profile::{QosLibrary, QosProfile, SingleOrSeq},
    },
    core::error::{DdsError, DdsResult},
};

macro_rules! impl_get_qos {
    ($fn_name:ident, $qos_type:ty, $named_type:ty, $field:ident) => {
        pub(crate) fn $fn_name(
            &self,
            library_name: &str,
            profile_name: Option<&str>,
            qos_name: Option<&str>,
        ) -> Option<&$qos_type> {
            let qos_or_named = if let Some(profile_name) = profile_name {
                self.get_profile(library_name, profile_name)?.$field.as_ref()?
            } else {
                self.get_library(library_name)?.$field.as_ref()?
            };

            match qos_or_named {
                SingleOrSeq::Single(qos) => qos_name.is_none().then_some(qos),
                SingleOrSeq::Seq(seq) => {
                    let name = qos_name?;
                    seq.iter().find(|q| q.name == name).map(|q| &q.qos)
                }
            }
        }
    };
}

#[derive(Serialize, Deserialize)]
pub(crate) struct QosLoader {
    libraries: HashMap<String, QosLibrary>,
}

#[allow(dead_code)]
impl QosLoader {
    pub(crate) fn from_file(path: &Path) -> DdsResult<Self> {
        let content = fs::read_to_string(path)
            .map_err(|e| DdsError::Error(format!("Failed to read QoS file: {:?}", e)))?;
        Self::from_json(&content)
    }

    pub(crate) fn from_json(json: &str) -> DdsResult<Self> {
        serde_json::from_str(json)
            .or_else(|_| {
                serde_json::from_str::<QosLibrary>(json).map(|lib| {
                    let mut libraries = HashMap::new();
                    libraries.insert(lib.name.clone(), lib);
                    Self { libraries }
                })
            })
            .map_err(|e| DdsError::Error(format!("Failed to parse QoS JSON: {:?}", e)))
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

    impl_get_qos!(get_datawriter_qos, DataWriterQos, DataWriterQosNamed, datawriter_qos);
    impl_get_qos!(get_datareader_qos, DataReaderQos, DataReaderQosNamed, datareader_qos);
    impl_get_qos!(get_topic_qos, TopicQos, TopicQosNamed, topic_qos);
    impl_get_qos!(get_subscriber_qos, SubscriberQos, SubscriberQosNamed, subscriber_qos);
    impl_get_qos!(get_publisher_qos, PublisherQos, PublisherQosNamed, publisher_qos);
    impl_get_qos!(
        get_domainparticipant_qos,
        DomainParticipantQos,
        DomainParticipantQosNamed,
        domain_participant_qos
    );
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
}
