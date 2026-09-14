//! Shared-memory zero-copy data plane.
//!
//! A participant owns one segment: a pool of payload slots plus an MPSC ring
//! that carries 24-byte slot descriptors. A writer serializes into a slot and
//! sends the descriptor; the reader claims the slot and reads it in place.
//! Discovery stays on UDP, and so does any sample no slot could take.
//!
//! - [`runtime`] - the segment, registry slot and peer map a participant owns
//! - [`pool_owner`] / [`pool_reader`] - the two ends of a payload slot
//! - [`ring`] - the descriptor queue
//! - [`recv`] - the loop that drains it
//! - [`platform`] - shared-memory primitives per OS

pub(crate) mod config;
pub(crate) mod fallback;
pub(crate) mod layout;
pub(crate) mod notify;
pub(crate) mod participant_slot;
pub(crate) mod peer_map;
pub(crate) mod platform;
pub(crate) mod pool;
pub(crate) mod pool_owner;
pub(crate) mod pool_reader;
pub(crate) mod recv;
pub(crate) mod registry;
pub(crate) mod registry_segment;
pub(crate) mod ring;
pub(crate) mod runtime;
pub(crate) mod segment;
pub(crate) mod shm_transport_plugin;
pub(crate) mod slot_handle;
pub(crate) mod slot_ref;

#[cfg(test)]
mod integration_test;

#[cfg(test)]
pub(crate) mod test_region;
