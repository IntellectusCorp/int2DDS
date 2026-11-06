//! DomainParticipantListener - Callback interface for domain participant events.
//!
//! The `DomainParticipantListener` trait allows applications to receive asynchronous
//! notifications about status changes related to a `DomainParticipant` and its child entities.
//!
//! This listener trait combines all the listener capabilities of Topic, Publisher, and
//! Subscriber entities, allowing a single listener to handle events from all entities
//! within a domain participant. A child entity's status is delivered to its own listener,
//! its parent's listener, and this listener, each gated by the child's status mask.

use crate::{
    publication::publisher_listener::PublisherListener,
    subscription::subscriber_listener::SubscriberListener, topic::topic_listener::TopicListener,
};

pub trait DomainParticipantListener:
    TopicListener + SubscriberListener + PublisherListener
{
}
