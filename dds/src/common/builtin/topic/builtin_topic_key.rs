//! Builtin topic key type for DDS discovery data.
//!
//! The `BuiltinTopicKey` uniquely identifies entities in DDS builtin topics
//! used for discovery (participants, publications, subscriptions, topics).

use crate::dcps::topic::type_support::DdsType;

#[derive(DdsType, Eq)]
#[dds_type(crate_path = "crate")]
pub struct BuiltinTopicKey {
    pub value: [i32; 3],
}
