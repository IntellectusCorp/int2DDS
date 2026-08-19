// Route-gateway relay integration tests (route_gateway::TopicRelay / AutoRelay).

mod common;

#[path = "relay/auto.rs"]
mod auto;

#[path = "relay/topic.rs"]
mod topic;
