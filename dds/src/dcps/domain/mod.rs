//! Domain management - Entry point for DDS applications.
//!
//! This module provides the domain participant infrastructure, which is the starting
//! point for all DDS applications. A domain represents a separate communication plane,
//! and participants within a domain can discover and communicate with each other.
//!
//! # Key Components
//!
//! - [`domain_participant_factory`] - Singleton factory for creating domain participants
//! - [`domain_participant`] - The main entry point for creating DDS entities (topics, publishers, subscribers)
//! - [`domain_participant_listener`] - Aggregated listener interface for all entity-level status changes within the domain participant
//! - [`qos`] - Quality of Service policies for domain participants and factory
//!
//! # Typical Usage Flow
//!
//! 1. Get the factory singleton: `DomainParticipantFactory::get_instance()`
//! 2. Create a participant in a specific domain
//! 3. Use the participant to create topics, publishers, and subscribers
//! 4. Delete entities and participant when done

pub mod domain_participant;
pub mod domain_participant_factory;
pub mod domain_participant_listener;
pub mod qos;
