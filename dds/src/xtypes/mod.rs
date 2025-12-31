//! DDS-XTYPES (Extensible Types) support.
//!
//! This module provides type metadata and type compatibility checking
//! as defined in the DDS-XTYPES specification.
//!
//! # Key Components
//!
//! - [`TypeObject`] - Complete type description
//! - [`TypeIdentifier`] - Compact type reference with hash
//! - [`EquivalenceHash`] - 14-byte MD5-based type hash
//!
//! # Type Compatibility
//!
//! DDS-XTYPES enables type evolution and compatibility checking between
//! different versions of the same type. The [`TypeObject`] contains full
//! type information while [`TypeIdentifier`] provides a compact hash for
//! efficient matching.

mod type_object;

pub use type_object::*;
