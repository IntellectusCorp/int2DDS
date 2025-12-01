//! PAD submessage for message alignment.
//!
//! This module implements the PAD submessage used for aligning subsequent submessages
//! to specific byte boundaries in RTPS messages.

use speedy::{Readable, Writable};

#[derive(Debug, Clone, PartialEq, Eq, Readable, Writable)]
pub(crate) struct Pad {}

#[allow(dead_code)]
impl Pad {
    pub(crate) fn new(&self) -> Self {
        Self {}
    }

    pub(crate) fn is_valid(&self) -> bool {
        true
    }
}
