//! Checksum types for RTPS message integrity verification.
//!
//! This module defines checksum types used in RTPS submessages to verify data integrity.
//! Supports 32-bit, 64-bit, and 128-bit checksum formats.

pub type Checksum32 = [u8; 4];
pub type Checksum64 = [u8; 8];
pub type Checksum128 = [u8; 16];

#[derive(Debug, Clone, Copy)]
pub enum Checksum {
    Checksum32(Checksum32),
    Checksum64(Checksum64),
    Checksum128(Checksum128),
    ChecksumInvalid,
}
