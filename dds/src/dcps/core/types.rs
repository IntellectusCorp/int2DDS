//! Core type definitions and constants for DDS.
//!
//! This module provides fundamental type aliases and constants used throughout the DDS API.
//! It includes domain and participant identifiers, resource limit constants, and internal
//! state types.
use serde::de;
use serde::{Deserialize, Deserializer, Serializer};

pub const LENGTH_UNLIMITED: i32 = -1;

pub(crate) fn deserialize_i32_or_unlimited<'de, D>(deserializer: D) -> Result<i32, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum IntOrString {
        Int(i32),
        String(String),
    }

    match IntOrString::deserialize(deserializer)? {
        IntOrString::Int(v) => Ok(v),
        IntOrString::String(s) => match s.as_str() {
            "LENGTH_UNLIMITED" => Ok(LENGTH_UNLIMITED),
            _ => Err(de::Error::custom(format!("invalid constant: {}", s))),
        },
    }
}

pub(crate) fn serialize_i32_or_unlimited<S>(value: &i32, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    if *value == LENGTH_UNLIMITED {
        serializer.serialize_str("LENGTH_UNLIMITED")
    } else {
        serializer.serialize_i32(*value)
    }
}

pub type DomainId = i32;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum InstanceState {
    Registered,
    Unregistered,
    Disposed,
}
