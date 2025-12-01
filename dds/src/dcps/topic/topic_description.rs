//! TopicDescription - Common interface for topic-like entities.
//!
//! The `TopicDescription` trait provides a common interface for accessing basic information
//! about topics and topic-like entities (such as ContentFilteredTopic and MultiTopic).
//! It allows retrieval of the topic name, type name, and associated DomainParticipant.
//!
//! This trait is implemented by `Topic`, `ContentFilteredTopic`, and `MultiTopic`, providing
//! a uniform way to query their properties regardless of the specific topic variant.

use std::any::Any;

use crate::{
    common::instance_handle::InstanceHandle, core::error::DdsResult,
    domain::domain_participant::DomainParticipant,
};

#[allow(private_bounds)]
pub trait TopicDescription: TopicDescriptionInternal {
    fn get_participant(&self) -> DdsResult<DomainParticipant>;
    fn get_type_name(&self) -> &str;
    fn get_name(&self) -> &str;
}
pub(crate) trait TopicDescriptionInternal {
    fn as_any(&self) -> &dyn Any;
    fn topic_instance_handle(&self) -> DdsResult<InstanceHandle>;
}

// Common implementation, wrapper
macro_rules! impl_topic_description_impl {
    ($type:ty) => {
        impl TopicDescription for $type {
            fn get_participant(&self) -> DdsResult<DomainParticipant> {
                if let Some(weak_ref) = self.participant.as_ref() {
                    // Attempt to upgrade Weak<T> to Arc<T>
                    if let Some(participant_arc) = weak_ref.upgrade() {
                        return Ok((*participant_arc).clone());
                    }
                }

                // If participant is None or the reference has expired
                Err(DdsError::Error("Participant reference is invalid or expired".to_string()))
            }
            fn get_type_name(&self) -> &str {
                &self.type_name
            }
            fn get_name(&self) -> &str {
                &self.topic_name
            }
        }
        impl TopicDescriptionInternal for $type {
            fn as_any(&self) -> &dyn Any {
                self
            }
            fn topic_instance_handle(&self) -> DdsResult<InstanceHandle> {
                Ok(InstanceHandle::from_guid(&self.guid))
            }
        }
        impl $type {
            /// Wrapper for Condition::get_trigger_value()
            #[inline]
            pub fn get_participant(&self) -> DdsResult<DomainParticipant> {
                <Self as TopicDescription>::get_participant(self)
            }
            #[inline]
            pub fn get_type_name(&self) -> &str {
                <Self as TopicDescription>::get_type_name(self)
            }
            #[inline]
            pub fn get_name(&self) -> &str {
                <Self as TopicDescription>::get_name(self)
            }
        }
    };
}

macro_rules! impl_topic_description {
    ($type:ty) => {
        impl_topic_description_impl!($type);
    };
}

pub(crate) use impl_topic_description;
pub(crate) use impl_topic_description_impl;
