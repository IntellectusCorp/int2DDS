//! DDS standard QoS policy types for JSON/XML serialization.
//!
//! These types conform to the OMG DDS-JSON specification and provide
//! conversion to/from internal QoS types in [`crate::infrastructure::qos_policy`].

use crate::core::types::LENGTH_UNLIMITED;
use serde::de;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

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
