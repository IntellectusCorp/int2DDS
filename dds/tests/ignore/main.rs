// ignore_participant, ignore_publication, ignore_subscription and ignore_topic integration tests.

#[path = "../common/mod.rs"]
mod common;

mod participant;
mod publication;
mod subscription;

use std::{
    sync::Mutex,
    thread::sleep,
    time::{Duration as StdDuration, Instant},
};

use int2dds::dcps::{
    domain::{
        domain_participant::DomainParticipant,
        domain_participant_factory::DomainParticipantFactory,
        qos::{DomainParticipantFactoryQos, DomainParticipantQos},
    },
    infrastructure::{qos_policy::EntityFactoryQosPolicy, status::StatusMask},
};

const DISCOVERY_TIMEOUT: StdDuration = StdDuration::from_secs(10);

const POLL_INTERVAL: StdDuration = StdDuration::from_millis(50);

// Serializes participant creation with the test that switches the factory autoenable QoS.
static FACTORY_QOS_LOCK: Mutex<()> = Mutex::new(());

fn create_participant(domain_id: i32) -> DomainParticipant {
    let _guard = FACTORY_QOS_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    DomainParticipantFactory::get_instance()
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap()
}

fn create_disabled_participant(domain_id: i32) -> DomainParticipant {
    let _guard = FACTORY_QOS_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let factory = DomainParticipantFactory::get_instance();

    factory
        .set_qos(DomainParticipantFactoryQos {
            entity_factory: EntityFactoryQosPolicy { autoenable_created_entities: false },
        })
        .unwrap();

    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    factory.set_qos(DomainParticipantFactoryQos::default()).unwrap();

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
