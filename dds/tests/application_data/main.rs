// Application data QoS integration tests (USER_DATA, TOPIC_DATA, GROUP_DATA propagation).

#[path = "../common/mod.rs"]
mod common;

mod group_data;
mod topic_data;
mod user_data;

use std::{
    thread::sleep,
    time::{Duration as StdDuration, Instant},
};

use int2dds::{
    domain::{
        domain_participant::DomainParticipant,
        domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos,
    },
    infrastructure::status::StatusMask,
};

const DISCOVERY_TIMEOUT: StdDuration = StdDuration::from_secs(10);

const POLL_INTERVAL: StdDuration = StdDuration::from_millis(50);

// Enables the participant regardless of the factory autoenable setting.
fn create_participant(domain_id: i32, qos: DomainParticipantQos) -> DomainParticipant {
    let participant = DomainParticipantFactory::get_instance()
        .create_participant(domain_id, qos, None, StatusMask::default())
        .unwrap();

    participant.enable().unwrap();
    participant
}

fn delete_participant(participant: DomainParticipant) {
    participant.delete_contained_entities().unwrap();
    DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
}

// Polls until `find` returns Some or the deadline passes.
fn wait_for<T>(mut find: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = Instant::now() + DISCOVERY_TIMEOUT;

    while Instant::now() < deadline {
        if let Some(found) = find() {
            return Some(found);
        }
        sleep(POLL_INTERVAL);
    }

    None
}
