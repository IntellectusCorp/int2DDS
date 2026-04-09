//! Domain Participant Factory - The entry point for creating DDS participants.
//!
//! The `DomainParticipantFactory` is a singleton that serves as the factory for creating
//! `DomainParticipant` objects. It is the starting point for any DDS application.
//!
//! # Overview
//!
//! A DDS domain represents a separate communication plane. Participants in different domains
//! cannot communicate with each other. The factory manages all participants across all domains
//! within an application.
//!
//! # Basic Usage
//!
//! ```no_run
//! use int2dds::domain::domain_participant_factory::DomainParticipantFactory;
//! use int2dds::domain::qos::DomainParticipantQos;
//! use int2dds::infrastructure::status::StatusMask;
//!
//! // Get the factory singleton
//! let factory = DomainParticipantFactory::get_instance();
//!
//! // Create a participant in domain 0
//! let participant = factory.create_participant(
//!     0,  // domain_id
//!     DomainParticipantQos::default(),
//!     None,  // no listener
//!     StatusMask::default()
//! )?;
//!
//! // Use the participant to create topics, publishers, and subscribers
//! // ...
//!
//! // Clean up
//! factory.delete_participant(participant)?;
//! # Ok::<(), int2dds::core::error::DdsError>(())
//! ```
//!
//! # Key Concepts
//!
//! - **Singleton Pattern**: Only one factory instance exists per application
//! - **Domain Isolation**: Participants in different domains cannot communicate
//! - **Resource Management**: The factory tracks all created participants
//! - **Lifecycle Management**: Participants must be explicitly deleted

use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, LazyLock, Mutex, Weak},
};

use crate::{
    common::{
        env::{get_default_qos_profile, get_qos_profile_paths, init_from_env, DEFAULT_DOMAIN_ID},
        instance_handle::InstanceHandle,
    },
    config::json::QosProvider,
    core::{
        error::{DdsError, DdsResult},
        types::DomainId,
    },
    infrastructure::{qos_kind::QosKind, qos_policy::Qos, status::StatusMask},
    publication::qos::{DataWriterQos, PublisherQos},
    subscription::qos::{DataReaderQos, SubscriberQos},
    topic::qos::TopicQos,
};

use super::{
    domain_participant::DomainParticipant,
    domain_participant_listener::DomainParticipantListener,
    qos::{DomainParticipantFactoryQos, DomainParticipantQos},
};

#[derive(Default)]
pub struct DomainParticipantFactory {
    participants: Mutex<HashMap<DomainId, Vec<Weak<DomainParticipant>>>>,
    orphaned_participants: Arc<Mutex<Vec<Arc<DomainParticipant>>>>,
    qos: Mutex<DomainParticipantFactoryQos>,
    default_participant_qos: Mutex<Option<DomainParticipantQos>>,
    qos_provider: Mutex<QosProvider>,
}

impl DomainParticipantFactory {
    /// Creates a new `DomainParticipant` in the specified domain.
    ///
    /// The `DomainParticipant` is the entry point for DDS operations. It represents the
    /// participation of the application in a DDS domain. Through the participant, you can
    /// create topics, publishers, and subscribers.
    ///
    /// The participant will be automatically enabled if the factory's QoS policy
    /// `autoenable_created_entities` is set to true (which is the default).
    ///
    /// # Arguments
    ///
    /// * `domain_id` - The domain ID to join. Participants in different domains cannot communicate.
    /// * `qos_list` - Quality of Service policies for the participant. Use `DomainParticipantQos::default()` for defaults.
    /// * `listener` - Optional listener for status notifications. Pass `None` if not needed.
    /// * `mask` - Status mask indicating which status changes trigger listener callbacks. Use `StatusMask::default()` to disable all.
    ///
    /// # Returns
    ///
    /// Returns `Ok(DomainParticipant)` on success, or a `DdsError` if creation fails.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// * The QoS policies are inconsistent or unsupported
    /// * System resources are insufficient
    /// * Network initialization fails
    pub fn create_participant(
        &self,
        domain_id: DomainId,
        qos_list: impl Into<QosKind<DomainParticipantQos>>,
        listener: Option<Arc<dyn DomainParticipantListener>>,
        mask: StatusMask,
    ) -> DdsResult<DomainParticipant> {
        let domain_id = if domain_id != DEFAULT_DOMAIN_ID {
            domain_id
        } else {
            std::env::var("DDS_DOMAIN_ID")
                .ok()
                .and_then(|v| v.parse::<DomainId>().ok())
                .unwrap_or_else(|| {
                    log::warn!("DDS_DOMAIN_ID is not set or invalid, defaulting to 0");
                    0
                })
        };

        // Resolution chain for QosKind::Default: registered default → configured
        // default profile → spec default. QosKind::Specific is used as-is.
        let qos_list = match qos_list.into() {
            QosKind::Specific(q) => q,
            QosKind::Default => {
                if let Some(registered) =
                    self.default_participant_qos.lock().ok().and_then(|g| g.clone())
                {
                    registered
                } else if let Ok(profile_qos) = self.get_participant_qos_from_profile("") {
                    profile_qos
                } else {
                    DomainParticipantQos::default()
                }
            }
        };

        let participant =
            DomainParticipant::new(false, domain_id, qos_list.clone(), listener, mask)?;
        if self.get_qos()?.entity_factory.autoenable_created_entities {
            participant.enable()?;
        }
        let participant_ref = participant
            .self_ref
            .as_ref()
            .ok_or(DdsError::Error("Participant is not properly initialized".to_string()))?
            .clone();
        let weak_participant = Arc::downgrade(&participant_ref);
        {
            let mut map_guard =
                self.participants.lock().map_err(|e| DdsError::Error(e.to_string()))?;
            let domain_participants = map_guard.entry(domain_id).or_insert_with(Vec::new);
            domain_participants.push(weak_participant);
        }

        Ok(participant)
    }

    /// Deletes a `DomainParticipant` and releases all associated resources.
    ///
    /// This method will delete the specified participant and clean up all associated resources.
    /// Before deleting, all entities created by this participant (topics, publishers, subscribers,
    /// data writers, data readers) must be deleted first.
    ///
    /// # Arguments
    ///
    /// * `participant` - The `DomainParticipant` to delete
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success, or a `DdsError` if deletion fails.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// * The participant still has active entities (topics, publishers, subscribers, etc.).
    ///   All entities must be deleted before deleting the participant.
    /// * The participant was not found in the factory's registry
    /// * Internal synchronization fails
    pub fn delete_participant(&self, mut participant: DomainParticipant) -> DdsResult<()> {
        match self.try_delete_participant(&mut participant) {
            Ok(()) => Ok(()),
            Err(e) => {
                let participant_ref = participant
                    .self_ref
                    .as_ref()
                    .ok_or(DdsError::Error("Participant is not properly initialized".to_string()))?
                    .clone();
                let mut orphaned = self
                    .orphaned_participants
                    .lock()
                    .map_err(|e| DdsError::Error(e.to_string()))?;
                if !orphaned.iter().any(|p| Arc::ptr_eq(p, &participant_ref)) {
                    orphaned.push(participant_ref);
                }
                Err(e)
            }
        }
    }

    pub(crate) fn try_delete_participant(
        &self,
        participant: &mut DomainParticipant,
    ) -> DdsResult<()> {
        self.remove_orphaned_participant(participant)?;
        if let Ok(true) = participant.has_active_entities() {
            return Err(DdsError::PreconditionNotMet);
        }

        let domain_id = participant.get_domain_id()?;
        match self.participants.lock() {
            Ok(mut map_guard) => {
                if let Some(domain_parts) = map_guard.get_mut(&domain_id) {
                    if let Some((pos, p)) =
                        domain_parts.iter().enumerate().find_map(|(i, weak_p)| {
                            weak_p.upgrade().and_then(|p| {
                                if *p == *participant {
                                    Some((i, p))
                                } else {
                                    None
                                }
                            })
                        })
                    {
                        participant.disable()?;
                        p.disable()?;
                        domain_parts.remove(pos);
                        if domain_parts.is_empty() {
                            map_guard.remove(&domain_id);
                        }
                        participant.delete()?;
                        Ok(())
                    } else {
                        Err(DdsError::Error("Participant not found".to_string()))
                    }
                } else {
                    Err(DdsError::Error("Domain not found".to_string()))
                }
            }
            Err(e) => Err(DdsError::Error(e.to_string())),
        }
    }

    fn remove_orphaned_participant(&self, target: &DomainParticipant) -> DdsResult<bool> {
        let mut orphaned_participants =
            self.orphaned_participants.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        if let Some(pos) = orphaned_participants.iter().enumerate().find_map(|(i, t)| {
            let handle = t.get_instance_handle().ok()?;
            let target_handle = target.get_instance_handle().ok()?;
            if handle == target_handle {
                Some(i)
            } else {
                None
            }
        }) {
            orphaned_participants.remove(pos);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub(crate) fn handle_participant_drop(
        &self,
        domain_id: &DomainId,
        participant_handle: &InstanceHandle,
    ) {
        let mut found_participant = None;
        if let Ok(participants) = self.participants.lock() {
            if let Some(participants) = participants.get(domain_id) {
                for weak_participant in participants.iter() {
                    if let Some(strong_participant) = weak_participant.upgrade() {
                        if let Ok(handle) = strong_participant.get_instance_handle() {
                            if handle == *participant_handle {
                                found_participant = Some(strong_participant);
                                break;
                            }
                        }
                    }
                }
            }
        }

        // If found, add DomainParticipant to orphaned_entities
        if let Some(participant) = found_participant {
            if let Ok(mut orphaned_participants) = self.orphaned_participants.lock() {
                if !orphaned_participants.iter().any(|p| Arc::ptr_eq(p, &participant)) {
                    orphaned_participants.push(participant);
                }
            }
        }
        // If not found, it has already been properly deleted via delete_participant, so do nothing
    }

    pub fn lookup_participant(&self, domain_id: DomainId) -> DdsResult<Option<DomainParticipant>> {
        let map_guard = self.participants.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        Ok(map_guard
            .get(&domain_id)
            .and_then(|participants| participants.first())
            .and_then(|weak_p| weak_p.upgrade())
            .map(|p| (*p).clone()))
    }

    pub fn lookup_participants(
        &self,
        domain_id: DomainId,
    ) -> DdsResult<Option<Vec<DomainParticipant>>> {
        let map_guard = self.participants.lock().map_err(|e| DdsError::Error(e.to_string()))?;

        Ok(map_guard.get(&domain_id).map(|participants| {
            participants
                .iter()
                .filter_map(|weak_p| weak_p.upgrade())
                .map(|p| (*p).clone())
                .collect::<Vec<DomainParticipant>>()
        }))
    }

    /// Returns the singleton instance of the `DomainParticipantFactory`.
    ///
    /// This is the entry point for creating DDS domain participants. The factory is implemented
    /// as a thread-safe singleton, ensuring that all parts of your application share the same
    /// factory instance.
    ///
    /// # Returns
    ///
    /// A static reference to the `DomainParticipantFactory` singleton instance.
    pub fn get_instance() -> &'static Self {
        static INSTANCE: LazyLock<DomainParticipantFactory> = LazyLock::new(|| {
            init_from_env();
            let factory = DomainParticipantFactory::default();

            // Auto-load QoS profiles from DDS_QOS_PROFILE env var
            let paths = get_qos_profile_paths();
            if !paths.is_empty() {
                match factory.load_profiles(&paths) {
                    Ok(()) => {
                        for p in &paths {
                            log::info!("Auto-loaded QoS profile: {}", p.display());
                        }
                    }
                    Err(e) => {
                        log::warn!(
                            "Failed to auto-load QoS profiles from DDS_QOS_PROFILE: {:?}",
                            e
                        );
                    }
                }
            }

            factory
        });
        &INSTANCE
    }

    pub fn set_qos(&self, qos: DomainParticipantFactoryQos) -> DdsResult<()> {
        qos.check_unsupported_policies()?;
        qos.is_consistent()?;

        {
            match self.qos.lock() {
                Ok(mut factory_qos) => {
                    *factory_qos = qos;
                    Ok(())
                }
                Err(e) => Err(DdsError::Error(e.to_string())),
            }
        }
    }

    pub fn get_qos(&self) -> DdsResult<DomainParticipantFactoryQos> {
        match self.qos.lock() {
            Ok(qos) => Ok(qos.clone()),
            Err(e) => Err(DdsError::Error(e.to_string())),
        }
    }

    pub fn set_default_participant_qos(
        &self,
        qos: impl Into<QosKind<DomainParticipantQos>>,
    ) -> DdsResult<()> {
        match qos.into() {
            QosKind::Default => match self.default_participant_qos.lock() {
                Ok(mut default_qos) => {
                    *default_qos = None;
                    Ok(())
                }
                Err(e) => Err(DdsError::Error(e.to_string())),
            },
            QosKind::Specific(qos) => {
                qos.is_consistent()?;
                match self.default_participant_qos.lock() {
                    Ok(mut default_qos) => {
                        *default_qos = Some(qos);
                        Ok(())
                    }
                    Err(e) => Err(DdsError::Error(e.to_string())),
                }
            }
        }
    }

    pub fn get_default_participant_qos(&self) -> DdsResult<DomainParticipantQos> {
        Ok(self
            .default_participant_qos
            .lock()
            .map_err(|e| DdsError::Error(e.to_string()))?
            .clone()
            .unwrap_or_default())
    }

    // ========== QoS Profile methods ==========

    /// Loads QoS profiles from one or more JSON files.
    ///
    /// The loaded profiles can be used with `create_participant_with_profile` and
    /// `get_*_qos_from_profile` methods to create entities with predefined QoS settings.
    ///
    /// Multiple files can be loaded incrementally. If a library with the same name
    /// already exists, it will be replaced by the new one.
    ///
    /// # Arguments
    /// * `paths` - Slice of file paths to load QoS profiles from
    ///
    /// # Errors
    /// Returns an error if any file cannot be read or parsed.
    pub fn load_profiles<P: AsRef<Path>>(&self, paths: &[P]) -> DdsResult<()> {
        let mut provider = self.qos_provider.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        for path in paths {
            provider.load_file(path.as_ref())?;
        }
        Ok(())
    }

    /// Creates a new `DomainParticipant` using QoS settings from a loaded profile.
    ///
    /// # Arguments
    /// * `domain_id` - The domain ID to join
    /// * `qos_path` - QoS path in the format `"Library::Profile"` or `"Library::Profile::QosName"`
    /// * `listener` - Optional listener for status notifications
    /// * `mask` - Status mask indicating which status changes trigger listener callbacks
    ///
    /// # Errors
    /// Returns an error if the profile is not found or participant creation fails.
    pub fn create_participant_with_profile(
        &self,
        domain_id: DomainId,
        qos_path: &str,
        listener: Option<Arc<dyn DomainParticipantListener>>,
        mask: StatusMask,
    ) -> DdsResult<DomainParticipant> {
        let qos = self.get_participant_qos_from_profile(qos_path)?;
        self.create_participant(domain_id, qos, listener, mask)
    }

    /// Returns the default QoS profile path (`"Library::Profile"`) to be applied
    /// when an entity is created via `*_with_profile("")`.
    ///
    /// Resolution order:
    /// 1. `DDS_DEFAULT_QOS_PROFILE` environment variable
    /// 2. The first profile in the loaded provider with `is_default_profile = true`
    /// 3. `None` (callers fall back to library/profile defaults)
    pub fn default_profile_path(&self) -> Option<String> {
        if let Some(p) = get_default_qos_profile() {
            return Some(p);
        }
        let provider = self.qos_provider.lock().ok()?;
        provider.default_profile_path()
    }

    /// Resolve an effective QoS path:
    /// - non-empty `qos_path` → returned unchanged.
    /// - empty `qos_path` + a configured default profile → return that default.
    /// - empty `qos_path` + no default profile → `Err` (callers fall through to spec defaults).
    ///
    /// This deliberately does NOT fall back to the provider's "first available" lookup,
    /// because that would silently apply an arbitrary profile when the user intended none.
    fn resolve_profile_path(&self, qos_path: &str) -> DdsResult<String> {
        if qos_path.is_empty() {
            self.default_profile_path()
                .ok_or_else(|| DdsError::Error("No default QoS profile configured".to_string()))
        } else {
            Ok(qos_path.to_string())
        }
    }

    /// Retrieves `DomainParticipantQos` from a loaded profile.
    ///
    /// # Arguments
    /// * `qos_path` - QoS path. See [`QosProvider`](crate::config::json::QosProvider) for supported formats.
    ///   Pass `""` to use the default profile (resolved via `default_profile_path()`).
    ///
    /// # Errors
    /// Returns an error if the profile is not found.
    pub fn get_participant_qos_from_profile(
        &self,
        qos_path: &str,
    ) -> DdsResult<DomainParticipantQos> {
        let resolved = self.resolve_profile_path(qos_path)?;
        let provider = self.qos_provider.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        provider
            .get_domainparticipant_qos(&resolved)
            .ok_or_else(|| DdsError::Error(format!("QoS profile not found: {}", resolved)))
    }

    /// Retrieves `PublisherQos` from a loaded profile.
    ///
    /// # Arguments
    /// * `qos_path` - QoS path. See [`QosProvider`](crate::config::json::QosProvider) for supported formats.
    ///
    /// # Errors
    /// Returns an error if the profile is not found.
    pub fn get_publisher_qos_from_profile(&self, qos_path: &str) -> DdsResult<PublisherQos> {
        let resolved = self.resolve_profile_path(qos_path)?;
        let provider = self.qos_provider.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        provider
            .get_publisher_qos(&resolved)
            .ok_or_else(|| DdsError::Error(format!("QoS profile not found: {}", resolved)))
    }

    /// Retrieves `SubscriberQos` from a loaded profile.
    ///
    /// # Arguments
    /// * `qos_path` - QoS path. See [`QosProvider`](crate::config::json::QosProvider) for supported formats.
    ///
    /// # Errors
    /// Returns an error if the profile is not found.
    pub fn get_subscriber_qos_from_profile(&self, qos_path: &str) -> DdsResult<SubscriberQos> {
        let resolved = self.resolve_profile_path(qos_path)?;
        let provider = self.qos_provider.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        provider
            .get_subscriber_qos(&resolved)
            .ok_or_else(|| DdsError::Error(format!("QoS profile not found: {}", resolved)))
    }

    /// Retrieves `TopicQos` from a loaded profile.
    ///
    /// # Arguments
    /// * `qos_path` - QoS path. See [`QosProvider`](crate::config::json::QosProvider) for supported formats.
    ///
    /// # Errors
    /// Returns an error if the profile is not found.
    pub fn get_topic_qos_from_profile(&self, qos_path: &str) -> DdsResult<TopicQos> {
        let resolved = self.resolve_profile_path(qos_path)?;
        let provider = self.qos_provider.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        provider
            .get_topic_qos(&resolved)
            .ok_or_else(|| DdsError::Error(format!("QoS profile not found: {}", resolved)))
    }

    /// Retrieves `DataWriterQos` from a loaded profile.
    ///
    /// # Arguments
    /// * `qos_path` - QoS path. See [`QosProvider`](crate::config::json::QosProvider) for supported formats.
    ///
    /// # Errors
    /// Returns an error if the profile is not found.
    pub fn get_datawriter_qos_from_profile(&self, qos_path: &str) -> DdsResult<DataWriterQos> {
        let resolved = self.resolve_profile_path(qos_path)?;
        let provider = self.qos_provider.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        provider
            .get_datawriter_qos(&resolved)
            .ok_or_else(|| DdsError::Error(format!("QoS profile not found: {}", resolved)))
    }

    /// Retrieves `DataReaderQos` from a loaded profile.
    ///
    /// # Arguments
    /// * `qos_path` - QoS path. See [`QosProvider`](crate::config::json::QosProvider) for supported formats.
    ///
    /// # Errors
    /// Returns an error if the profile is not found.
    pub fn get_datareader_qos_from_profile(&self, qos_path: &str) -> DdsResult<DataReaderQos> {
        let resolved = self.resolve_profile_path(qos_path)?;
        let provider = self.qos_provider.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        provider
            .get_datareader_qos(&resolved)
            .ok_or_else(|| DdsError::Error(format!("QoS profile not found: {}", resolved)))
    }
}

#[cfg(test)]
mod factory_test {
    use crate::{
        infrastructure::qos_policy::{EntityFactoryQosPolicy, UserDataQosPolicy},
        publication::qos::PublisherQos,
    };

    use super::*;

    #[test]
    fn test_singleton_instance() {
        let instance1 = DomainParticipantFactory::get_instance();
        let instance2 = DomainParticipantFactory::get_instance();

        assert!(
            std::ptr::eq(instance1, instance2),
            "Singleton instances should have the same memory address"
        );

        // Change QoS value and verify it's reflected in other instances
        let test_qos = DomainParticipantQos {
            user_data: UserDataQosPolicy::default(),
            entity_factory: EntityFactoryQosPolicy { autoenable_created_entities: false },
        };
        instance1.set_default_participant_qos(test_qos).unwrap();

        let qos1 = instance1.get_default_participant_qos();
        let qos2 = instance2.get_default_participant_qos();

        assert_eq!(qos1, qos2, "QoS values should be the same across instances");
    }

    #[test]
    fn test_delete_participant() {
        let factory = DomainParticipantFactory::get_instance();
        let participant = factory
            .create_participant(10, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();

        let result = factory.lookup_participant(10).unwrap();
        assert!(result.is_some(), "is None");

        let result = factory.delete_participant(participant);
        println!("{:?}", result);
        assert_eq!(result, Ok(()));
        let result = factory.lookup_participant(10).unwrap();
        assert!(result.is_none(), "is not None");
    }

    #[test]
    fn test_participant_drop_without_delete() {
        let domain_participant_factory = DomainParticipantFactory::get_instance();
        let domain_participant = domain_participant_factory
            .create_participant(20, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();
        let domain_participant_cloned = domain_participant.clone();
        let domain_participant_cloned2 = domain_participant.clone();

        // DomainParticipant Entity should not be deleted even after being dropped
        drop(domain_participant);
        let contains_domain_participant =
            domain_participant_factory.lookup_participant(20).unwrap();
        assert!(contains_domain_participant.is_some(), "domain_participant deleted!");

        // DomainParticipant should be deleted when explicitly calling delete_participant
        domain_participant_factory.delete_participant(domain_participant_cloned).unwrap();
        let contains_domain_participant =
            domain_participant_factory.lookup_participant(20).unwrap();
        assert!(contains_domain_participant.is_none(), "domain_participant still exists!");

        // After participant deletion, operations on remaining clones should return AlreadyDeleted error
        let publisher = domain_participant_cloned2.create_publisher(
            PublisherQos::default(),
            None,
            StatusMask::default(),
        );
        if let Err(DdsError::AlreadyDeleted) = publisher {
        } else {
            panic!("Expected DdsError::AlreadyDeleted");
        }
    }
}
