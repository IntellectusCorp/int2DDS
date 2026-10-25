// Several threads take from one reader at the same time, one sample per call, the way a
// multi-threaded executor with a reentrant callback group does. Every sample must be handed out
// exactly once: nothing taken twice, nothing lost. Checked for the typed take, the serialized take
// and the serialized bytes take (the path behind int2dds_datareader_take_serialized_loaned).
mod common;

use common::*;
use int2dds::common::instance_handle::InstanceHandle;
use int2dds::dcps::{
    core::{error::DdsError, time::Duration},
    domain::{
        domain_participant::DomainParticipant,
        domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos,
    },
    infrastructure::{
        qos_policy::{
            HistoryQosPolicy, HistoryQosPolicyKind, ReliabilityQosPolicy, ReliabilityQosPolicyKind,
        },
        status::StatusMask,
    },
    publication::qos::{DataWriterQos, PublisherQos},
    subscription::{
        qos::{DataReaderQos, SubscriberQos},
        sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
    },
    topic::{qos::TopicQos, type_support::DdsType},
};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

const SAMPLES: i16 = 3000;
const THREADS: usize = 4;

#[derive(Clone, Copy, Debug)]
enum Take {
    Typed,
    Serialized,
    SerializedBytes,
}

fn participant(domain_id: i32) -> DomainParticipant {
    DomainParticipantFactory::get_instance()
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap()
}

// Writes SAMPLES distinct samples and takes them with THREADS threads at once. Returns every
// sample identity handed out (duplicates included).
fn run(take: Take) -> Vec<Vec<u8>> {
    let domain_id = next_domain_id();
    let wp = participant(domain_id);
    let rp = participant(domain_id);
    let rel = ReliabilityQosPolicy {
        kind: ReliabilityQosPolicyKind::Reliable,
        max_blocking_time: Duration { sec: 5, nanosec: 0 },
    };
    let keep_all = HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true };
    let name = format!("ConcurrentTake{take:?}");
    let wt = wp
        .create_topic::<NoKeyDataType>(
            &name,
            NoKeyDataType::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let publisher =
        wp.create_publisher(PublisherQos::default(), None, StatusMask::default()).unwrap();
    let writer = publisher
        .create_datawriter::<NoKeyDataType>(
            &wt,
            DataWriterQos {
                reliability: rel.clone(),
                history: keep_all.clone(),
                ..Default::default()
            },
            None,
            StatusMask::default(),
        )
        .unwrap();
    let rt = rp
        .create_topic::<NoKeyDataType>(
            &name,
            NoKeyDataType::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let subscriber =
        rp.create_subscriber(SubscriberQos::default(), None, StatusMask::default()).unwrap();
    let reader = subscriber
        .create_datareader::<NoKeyDataType>(
            &rt,
            DataReaderQos { reliability: rel, history: keep_all, ..Default::default() },
            None,
            StatusMask::default(),
        )
        .unwrap();
    wait_for_writer_status(
        &writer,
        StatusMask::PUBLICATION_MATCHED,
        Duration { sec: 10, nanosec: 0 },
    )
    .expect("match");

    let seen = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    let stop = Arc::new(AtomicBool::new(false));
    let workers: Vec<_> = (0..THREADS)
        .map(|_| {
            let reader = reader.clone();
            let seen = Arc::clone(&seen);
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                let states = (
                    [SampleStateKind::ANY_SAMPLE_STATE],
                    [ViewStateKind::ANY_VIEW_STATE],
                    [InstanceStateKind::ANY_INSTANCE_STATE],
                );
                while !stop.load(Ordering::Relaxed) {
                    let got: Vec<Vec<u8>> = match take {
                        Take::Typed => match reader.take(1, &states.0, &states.1, &states.2) {
                            Ok(samples) => samples
                                .iter()
                                .filter_map(|s| s.data().ok())
                                .map(|d| d.value.to_le_bytes().to_vec())
                                .collect(),
                            Err(DdsError::NoData) => Vec::new(),
                            Err(e) => panic!("typed take failed: {e:?}"),
                        },
                        Take::Serialized => {
                            match reader.take_serialized(1, &states.0, &states.1, &states.2) {
                                Ok(samples) => samples
                                    .iter()
                                    .filter(|(_, info)| info.valid_data)
                                    .map(|(bytes, _)| bytes.to_vec())
                                    .collect(),
                                Err(DdsError::NoData) => Vec::new(),
                                Err(e) => panic!("serialized take failed: {e:?}"),
                            }
                        }
                        Take::SerializedBytes => match reader.take_next_serialized_bytes() {
                            Ok((bytes, info)) if info.valid_data => vec![bytes.to_vec()],
                            Ok(_) | Err(DdsError::NoData) => Vec::new(),
                            Err(e) => panic!("serialized bytes take failed: {e:?}"),
                        },
                    };
                    if got.is_empty() {
                        std::thread::yield_now();
                    } else {
                        seen.lock().unwrap().extend(got);
                    }
                }
            })
        })
        .collect();

    for value in 0..SAMPLES {
        writer.write(&NoKeyDataType { value }, InstanceHandle::NIL).unwrap();
    }
    let deadline = Instant::now() + std::time::Duration::from_secs(30);
    while Instant::now() < deadline {
        let unique = seen.lock().unwrap().iter().collect::<HashSet<_>>().len();
        if unique >= SAMPLES as usize {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    // Let a duplicate that is being handed out right now land before counting.
    std::thread::sleep(std::time::Duration::from_millis(200));
    stop.store(true, Ordering::Relaxed);
    for worker in workers {
        worker.join().expect("a take thread panicked");
    }

    let taken = seen.lock().unwrap().clone();
    drop(reader);
    drop(writer);
    let factory = DomainParticipantFactory::get_instance();
    let _ = wp.delete_contained_entities();
    let _ = rp.delete_contained_entities();
    let _ = factory.delete_participant(wp);
    let _ = factory.delete_participant(rp);
    taken
}

fn check(take: Take) {
    let taken = run(take);
    let unique: HashSet<&Vec<u8>> = taken.iter().collect();
    assert_eq!(
        taken.len(),
        unique.len(),
        "{take:?}: {} samples handed out, {} distinct ({} duplicates)",
        taken.len(),
        unique.len(),
        taken.len() - unique.len()
    );
    assert_eq!(
        unique.len(),
        SAMPLES as usize,
        "{take:?}: {} of {SAMPLES} samples arrived",
        unique.len()
    );
}

#[test]
fn concurrent_typed_take_hands_each_sample_out_once() {
    check(Take::Typed);
}

#[test]
fn concurrent_serialized_take_hands_each_sample_out_once() {
    check(Take::Serialized);
}

#[test]
fn concurrent_serialized_bytes_take_hands_each_sample_out_once() {
    check(Take::SerializedBytes);
}
