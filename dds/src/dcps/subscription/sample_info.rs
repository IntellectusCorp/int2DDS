//! SampleInfo - Metadata accompanying received data samples.
//!
//! This module defines `SampleInfo` and related state types that provide metadata about
//! received data samples. SampleInfo accompanies each `DataSample` and contains information
//! about sample state, view state, instance state, timestamps, and source identification.
//!
//! # State Types
//!
//! - **SampleStateKind**: Whether the sample has been READ or NOT_READ
//! - **ViewStateKind**: Whether this is a NEW instance or NOT_NEW
//! - **InstanceStateKind**: Whether the instance is ALIVE, DISPOSED, or has NO_WRITERS
//!
//! These states are used for filtering in read/take operations and ReadConditions.

use std::{ops::BitOr, sync::Arc};

use crate::{common::instance_handle::InstanceHandle, core::time::Time};
use bitflags::bitflags;

pub trait StateMaskExt<T>
where
    T: bitflags::Flags + BitOr<Output = T> + Copy,
{
    fn matches(&self, state: T) -> bool;
}

impl<T> StateMaskExt<T> for [T]
where
    T: bitflags::Flags + BitOr<Output = T> + Copy,
{
    fn matches(&self, state: T) -> bool {
        let combined = self.iter().fold(T::empty(), |acc, &m| acc | m);
        combined.contains(state)
    }
}

// Sample states to support reads
bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct SampleStateKind: u32 {
        const READ_SAMPLE_STATE = 0x0001;
        const NOT_READ_SAMPLE_STATE = 0x0002;
        const ANY_SAMPLE_STATE = 0xffff;
    }
}

// View states to support reads
bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct ViewStateKind: u32 {
        const NEW_VIEW_STATE = 0x0001;
        const NOT_NEW_VIEW_STATE = 0x0002;
        const ANY_VIEW_STATE = 0xffff;
    }
}

// Instance states to support reads
bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct InstanceStateKind: u32 {
        const ALIVE_INSTANCE_STATE = 0x0001;
        const NOT_ALIVE_DISPOSED_INSTANCE_STATE = 0x0002;
        const NOT_ALIVE_NO_WRITERS_INSTANCE_STATE = 0x0004;
        const ANY_INSTANCE_STATE = 0xffff;
        const NOT_ALIVE_INSTANCE_STATE = 0x006;
    }
}

impl InstanceStateKind {
    // Convenient constructor methods
    pub fn not_alive() -> Self {
        Self::NOT_ALIVE_INSTANCE_STATE
    }

    pub fn any() -> Self {
        Self::ANY_INSTANCE_STATE
    }

    pub fn contains_state(self, state: InstanceStateKind) -> bool {
        (self.bits() & state.bits()) != 0
    }
}

// Conversion methods (for compatibility with existing code)
impl From<SampleStateKind> for u32 {
    fn from(state: SampleStateKind) -> Self {
        state.bits()
    }
}

impl From<u32> for SampleStateKind {
    fn from(bits: u32) -> Self {
        Self::from_bits_truncate(bits)
    }
}

impl From<ViewStateKind> for u32 {
    fn from(state: ViewStateKind) -> Self {
        state.bits()
    }
}

impl From<u32> for ViewStateKind {
    fn from(bits: u32) -> Self {
        Self::from_bits_truncate(bits)
    }
}

impl From<InstanceStateKind> for u32 {
    fn from(state: InstanceStateKind) -> Self {
        state.bits()
    }
}

impl From<u32> for InstanceStateKind {
    fn from(bits: u32) -> Self {
        Self::from_bits_truncate(bits)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SampleInfo {
    pub source_timestamp: Time,
    pub sample_state: SampleStateKind,
    pub view_state: ViewStateKind,
    pub instance_handle: InstanceHandle,
    pub instance_state: InstanceStateKind,
    pub disposed_generation_count: i32,   // long,
    pub no_writers_generation_count: i32, // long,
    pub absolute_generation_rank: i32,    //long
    pub sample_rank: i32,                 //long
    pub generation_rank: i32,             //long
    pub publication_handle: InstanceHandle,
    pub valid_data: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InstanceInfo {
    pub(crate) key: Arc<[u8]>,
    pub(crate) instance_state: InstanceStateKind,
    pub(crate) view_state: ViewStateKind,
    pub(crate) disposed_generation_count: i32,
    pub(crate) no_writers_generation_count: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sample_states() {
        let read_state = SampleStateKind::READ_SAMPLE_STATE;
        let not_read_state = SampleStateKind::NOT_READ_SAMPLE_STATE;
        let combined = read_state | not_read_state;

        assert!(combined.contains(SampleStateKind::READ_SAMPLE_STATE));
        assert!(combined.contains(SampleStateKind::NOT_READ_SAMPLE_STATE));
    }

    #[test]
    fn test_instance_states() {
        let not_alive = InstanceStateKind::not_alive();
        let disposed = InstanceStateKind::NOT_ALIVE_DISPOSED_INSTANCE_STATE;
        let no_writers = InstanceStateKind::NOT_ALIVE_NO_WRITERS_INSTANCE_STATE;
        let alive = InstanceStateKind::ALIVE_INSTANCE_STATE;

        assert!((not_alive.bits() & disposed.bits()) != 0);
        assert!((not_alive.bits() & no_writers.bits()) != 0);
        assert!((not_alive.bits() & alive.bits()) == 0);
    }

    #[test]
    fn test_instance_states_w_contains() {
        let not_alive = InstanceStateKind::not_alive();
        assert!(not_alive.contains_state(InstanceStateKind::NOT_ALIVE_DISPOSED_INSTANCE_STATE));
        assert!(not_alive.contains_state(InstanceStateKind::NOT_ALIVE_NO_WRITERS_INSTANCE_STATE));
        assert!(!not_alive.contains_state(InstanceStateKind::ALIVE_INSTANCE_STATE));
    }

    #[test]
    fn test_instance_states_w_intersects() {
        let not_alive = InstanceStateKind::NOT_ALIVE_INSTANCE_STATE;
        let disposed = InstanceStateKind::NOT_ALIVE_DISPOSED_INSTANCE_STATE;
        let no_writers = InstanceStateKind::NOT_ALIVE_NO_WRITERS_INSTANCE_STATE;

        assert!(not_alive.intersects(InstanceStateKind::from_bits_truncate(disposed.bits())));
        assert!(not_alive.intersects(InstanceStateKind::from_bits_truncate(no_writers.bits())));
    }

    #[test]
    fn test_conversion() {
        let state = SampleStateKind::READ_SAMPLE_STATE;
        let bits: u32 = state.into();
        let back: SampleStateKind = bits.into();
        assert_eq!(state, back);
    }
}
