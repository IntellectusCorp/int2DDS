//! TopicListener - Callback interface for topic status events.
//!
//! The `TopicListener` trait allows applications to receive asynchronous notifications
//! about status changes related to a `Topic`. Listeners are registered when creating
//! a topic or by calling `Topic::set_listener()`.
//!
//! Currently, the `TopicListener` provides callbacks for inconsistent topic detection,
//! which occurs when multiple topics with the same name but incompatible types or QoS
//! policies are discovered in the same domain.

use crate::infrastructure::status::InconsistentTopicStatus;

use super::topic::Topic;

pub trait TopicListener: 'static + Send + Sync {
    fn on_inconsistent_topic(&self, _topic: &Topic, _status: &InconsistentTopicStatus) {}
}
