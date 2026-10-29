//! TopicListener - Callback interface for topic status events.
//!
//! The `TopicListener` trait allows applications to receive asynchronous notifications
//! about status changes related to a `Topic`. Listeners are registered when creating
//! a topic or by calling `Topic::set_listener()`.
//!
//! Currently, the `TopicListener` provides callbacks for inconsistent topic detection,
//! which occurs when a remote endpoint on a topic with the same name disagrees on keyed
//! versus keyless, or fails the type compatibility check. QoS mismatches are reported as
//! incompatible QoS instead.

use crate::infrastructure::status::InconsistentTopicStatus;

use super::topic::Topic;

pub trait TopicListener: 'static + Send + Sync {
    fn on_inconsistent_topic(&self, _topic: &Topic, _status: &InconsistentTopicStatus) {}
}
