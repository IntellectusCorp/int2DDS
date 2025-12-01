//! Builtin topic key type for DDS discovery data.
//!
//! The `BuiltinTopicKey` uniquely identifies entities in DDS builtin topics
//! used for discovery (participants, publications, subscriptions, topics).

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltinTopicKey {
    pub value: [i32; 3],
}
