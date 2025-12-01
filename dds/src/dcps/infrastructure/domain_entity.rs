//! DomainEntity - Marker trait for entities belonging to a domain.
//!
//! The `DomainEntity` trait is a marker trait that identifies entities belonging to a
//! specific DDS domain (as opposed to the domain participant itself). This includes
//! Topics, Publishers, Subscribers, DataWriters, and DataReaders.

use super::entity::Entity;

pub trait DomainEntity: Entity {} // Concept to distinguish from DomainParticipant.
