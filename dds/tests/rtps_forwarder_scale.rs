//! What the forwarder costs as the two networks grow.
//!
//! The forwarder makes every participant discoverable to every participant on the
//! far network, and discovery is directed traffic, so what crosses the link
//! grows with the number of participant pairs rather than the number of
//! participants. Sweeping the participant count is what makes that visible;
//! sweeping the topic count on a single participant separates the endpoint
//! cost from the participant cost, because the two grow at different rates.

mod common;

use std::{thread::sleep, time::Duration as StdDuration};

use common::*;
use int2dds::{
    common::instance_handle::InstanceHandle,
    core::time::Duration,
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
    publication::{
        data_writer::DataWriter,
        qos::{DataWriterQos, PublisherQos},
    },
    rtps_forwarder::{Forwarder, ForwarderConfig, LinkRole, LinkStats},
    subscription::{
        data_reader::DataReader,
        qos::{DataReaderQos, SubscriberQos},
        sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
    },
    topic::qos::TopicQos,
};

const SAMPLE_COUNT: i16 = 5;
const MATCH_TIMEOUT_MS: u64 = 60_000;
const DELIVERY_TIMEOUT_MS: u64 = 30_000;

struct ForwarderPorts {
    metatraffic: u16,
    user_data: u16,
    link: u16,
}

impl ForwarderPorts {
    fn for_domain(domain_id: i32) -> Self {
        let base = 30000 + (domain_id as u16) * 4;
        Self { metatraffic: base, user_data: base + 1, link: base + 2 }
    }
}

/// One reading of the link under a given shape of the two networks.
struct Measurement {
    participants_per_side: usize,
    topics_each: usize,
    stats: LinkStats,
}

impl Measurement {
    fn endpoint_pairs(&self) -> usize {
        self.participants_per_side * self.topics_each
    }

    /// Discovery is directed traffic between every local and every remote
    /// participant, so the pair count rather than the participant count is what
    /// the cost should be divided by.
    fn metatraffic_per_pair(&self) -> u64 {
        let pairs = (self.participants_per_side * self.participants_per_side) as u64;
        self.stats.metatraffic_bytes() / pairs
    }

    fn report(&self) {
        println!(
            "{:>2} participants x {:>2} topics | discovery {:>8} B ({:>7} B/pair, {:>4} frames) | \
             user data {:>7} B ({:>4} frames)",
            self.participants_per_side,
            self.topics_each,
            self.stats.metatraffic_bytes(),
            self.metatraffic_per_pair(),
            self.stats.sent_metatraffic_frames + self.stats.received_metatraffic_frames,
            self.stats.user_data_bytes(),
            self.stats.sent_user_data_frames + self.stats.received_user_data_frames,
        );
    }
}

fn reliable_writer_qos() -> DataWriterQos {
    DataWriterQos {
        reliability: ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::Reliable,
            max_blocking_time: Duration { sec: 1, nanosec: 0 },
        },
        history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
        ..Default::default()
    }
}

fn reliable_reader_qos() -> DataReaderQos {
    DataReaderQos {
        reliability: ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::Reliable,
            max_blocking_time: Duration { sec: 1, nanosec: 0 },
        },
        history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
        ..Default::default()
    }
}

fn wait_until<F: FnMut() -> bool>(mut check: F, timeout_ms: u64) -> bool {
    let step = StdDuration::from_millis(100);
    let mut waited = 0u64;
    while waited < timeout_ms {
        if check() {
            return true;
        }
        sleep(step);
        waited += step.as_millis() as u64;
    }
    check()
}

fn create_writer(participant: &DomainParticipant, topic_name: &str) -> DataWriter<KeyedDataType> {
    let topic = participant
        .create_topic::<KeyedDataType>(
            topic_name,
            KeyedDataType::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let publisher =
        participant.create_publisher(PublisherQos::default(), None, StatusMask::default()).unwrap();
    publisher
        .create_datawriter::<KeyedDataType>(
            &topic,
            reliable_writer_qos(),
            None,
            StatusMask::default(),
        )
        .unwrap()
}

fn create_reader(participant: &DomainParticipant, topic_name: &str) -> DataReader<KeyedDataType> {
    let topic = participant
        .create_topic::<KeyedDataType>(
            topic_name,
            KeyedDataType::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();
    subscriber
        .create_datareader::<KeyedDataType>(
            &topic,
            reliable_reader_qos(),
            None,
            StatusMask::default(),
        )
        .unwrap()
}

/// Stands up both networks, waits for every pair to match, moves one round of
/// samples, and reads the link counters. Everything is torn down before the
/// reading is returned so that no participant of one measurement can be seen by
/// the next.
fn measure(participants_per_side: usize, topics_each: usize) -> Measurement {
    let domain_a = next_domain_id();
    let domain_b = next_domain_id();
    let ports_a = ForwarderPorts::for_domain(domain_a);
    let ports_b = ForwarderPorts::for_domain(domain_b);
    let link_address: std::net::SocketAddr = format!("127.0.0.1:{}", ports_a.link).parse().unwrap();

    let forwarder_a = Forwarder::start(ForwarderConfig::new(
        domain_a as u32,
        ports_a.metatraffic,
        ports_a.user_data,
        LinkRole::Listen(link_address),
    ))
    .expect("forwarder A failed to start");
    let forwarder_b = Forwarder::start(ForwarderConfig::new(
        domain_b as u32,
        ports_b.metatraffic,
        ports_b.user_data,
        LinkRole::Connect(link_address),
    ))
    .expect("forwarder B failed to start");

    let factory = DomainParticipantFactory::get_instance();
    let mut writer_participants = Vec::new();
    let mut reader_participants = Vec::new();
    let mut writers = Vec::new();
    let mut readers = Vec::new();

    for index in 0..participants_per_side {
        let writer_participant = factory
            .create_participant(
                domain_a,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let reader_participant = factory
            .create_participant(
                domain_b,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        // Each participant owns its own topics, so a writer only ever matches
        // the reader that stands opposite it across the link.
        for topic in 0..topics_each {
            let topic_name = format!("ScaleTopic_{}_{}", index, topic);
            writers.push(create_writer(&writer_participant, &topic_name));
            readers.push(create_reader(&reader_participant, &topic_name));
        }

        writer_participants.push(writer_participant);
        reader_participants.push(reader_participant);
    }

    let matched = wait_until(
        || {
            writers.iter().all(|writer| {
                writer
                    .get_publication_matched_status()
                    .map(|s| s.current_count() >= 1)
                    .unwrap_or(false)
            }) && readers.iter().all(|reader| {
                reader
                    .get_subscription_matched_status()
                    .map(|s| s.current_count() >= 1)
                    .unwrap_or(false)
            })
        },
        MATCH_TIMEOUT_MS,
    );
    assert!(
        matched,
        "not every pair matched at {} participants x {} topics",
        participants_per_side, topics_each
    );

    for writer in &writers {
        for value in 1..=SAMPLE_COUNT {
            writer.write(&KeyedDataType::new(1, value * 10), InstanceHandle::NIL).unwrap();
        }
    }

    let delivered = wait_until(
        || {
            readers.iter().all(|reader| {
                reader
                    .read(
                        100,
                        &[SampleStateKind::ANY_SAMPLE_STATE],
                        &[ViewStateKind::ANY_VIEW_STATE],
                        &[InstanceStateKind::ALIVE_INSTANCE_STATE],
                    )
                    .map(|samples| samples.len() >= SAMPLE_COUNT as usize)
                    .unwrap_or(false)
            })
        },
        DELIVERY_TIMEOUT_MS,
    );
    assert!(
        delivered,
        "not every reader received its samples at {} participants x {} topics",
        participants_per_side, topics_each
    );

    let stats = forwarder_a.link_stats();

    drop(writers);
    drop(readers);
    for participant in writer_participants {
        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
    }
    for participant in reader_participants {
        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
    }
    forwarder_a.stop();
    forwarder_b.stop();

    Measurement { participants_per_side, topics_each, stats }
}

/// Run with `--nocapture` to see the table. Discovery is quadratic in the
/// participant count because every local participant addresses every remote
/// one, and that is the limit of the design rather than a defect to fix here.
/// The assertions pin the exponent: the per pair cost has to stay flat as the
/// sweep grows, and a cubic term would break it.
#[test]
fn discovery_cost_is_quadratic_in_participants_and_cheap_in_topics() {
    let sweep: Vec<Measurement> = [1usize, 2, 4, 8].iter().map(|n| measure(*n, 1)).collect();
    let more_topics = measure(1, 4);

    println!("\nForwarder link cost");
    for measurement in &sweep {
        measurement.report();
    }
    more_topics.report();

    let baseline = &sweep[0];
    assert!(baseline.stats.metatraffic_bytes() > 0, "no discovery crossed the link");
    assert!(baseline.stats.user_data_bytes() > 0, "no user data crossed the link");

    // The band is wide on purpose. A single pair carries the fixed cost of
    // bringing the link up, so the per pair figure drifts down legitimately as
    // the sweep grows, and only a figure that climbs out of the band means the
    // cost per pair itself started growing.
    let baseline_per_pair = baseline.metatraffic_per_pair();
    for measurement in &sweep {
        let per_pair = measurement.metatraffic_per_pair();
        assert!(
            per_pair * 3 > baseline_per_pair && per_pair < baseline_per_pair * 2,
            "discovery per participant pair left the quadratic band at {} participants: \
             {} B/pair against {} B/pair at the baseline",
            measurement.participants_per_side,
            per_pair,
            baseline_per_pair
        );
    }

    // Topics ride on participants that already announce each other, so the same
    // endpoint count costs far less when it is stacked on one participant.
    assert_eq!(more_topics.endpoint_pairs(), sweep[2].endpoint_pairs());
    assert!(
        more_topics.stats.metatraffic_bytes() < sweep[2].stats.metatraffic_bytes(),
        "topics on one participant cost as much as separate participants: {} B against {} B",
        more_topics.stats.metatraffic_bytes(),
        sweep[2].stats.metatraffic_bytes()
    );
}
