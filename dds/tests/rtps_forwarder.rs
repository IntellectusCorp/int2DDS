//! End-to-end test of the transparent RTPS forwarder.
//!
//! ```text
//!   [Writer, domain A] ── multicast ──▶ [forwarder A] ══ TCP ══ [forwarder B] ──▶ [Reader, domain B]
//! ```
//! The two networks are kept apart by running them on different domains, so
//! nothing but the forwarder pair can carry discovery or data between them. The
//! writer and the reader are ordinary participants: neither knows a forwarder
//! exists.

mod common;

use std::{thread::sleep, time::Duration as StdDuration};

use common::*;
use int2dds::{
    common::instance_handle::InstanceHandle,
    core::time::Duration,
    domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
    infrastructure::{
        qos_policy::{
            HistoryQosPolicy, HistoryQosPolicyKind, ReliabilityQosPolicy, ReliabilityQosPolicyKind,
        },
        status::StatusMask,
    },
    publication::qos::{DataWriterQos, PublisherQos},
    rtps_forwarder::{Forwarder, ForwarderConfig, LinkRole},
    subscription::{
        qos::{DataReaderQos, SubscriberQos},
        sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
    },
};

const SAMPLE_COUNT: i16 = 5;

/// Ports for one forwarder, spaced so that concurrently running tests on
/// different domains never collide.
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

fn start_forwarder(domain_id: i32, ports: &ForwarderPorts, link: LinkRole) -> Forwarder {
    Forwarder::start(ForwarderConfig::new(
        domain_id as u32,
        ports.metatraffic,
        ports.user_data,
        link,
    ))
    .expect("forwarder failed to start")
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

#[test]
fn a_pair_of_forwarders_carries_discovery_and_samples_between_two_networks() {
    let domain_a = next_domain_id();
    let domain_b = next_domain_id();
    let ports_a = ForwarderPorts::for_domain(domain_a);
    let ports_b = ForwarderPorts::for_domain(domain_b);
    let link_address = format!("127.0.0.1:{}", ports_a.link);

    let forwarder_a =
        start_forwarder(domain_a, &ports_a, LinkRole::Listen(link_address.parse().unwrap()));
    let forwarder_b =
        start_forwarder(domain_b, &ports_b, LinkRole::Connect(link_address.parse().unwrap()));

    let factory = DomainParticipantFactory::get_instance();
    let writer_participant = factory
        .create_participant(domain_a, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();
    let writer =
        create_datawriter(&writer_participant, PublisherQos::default(), reliable_writer_qos());

    let reader_participant = factory
        .create_participant(domain_b, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();
    let reader =
        create_datareader(&reader_participant, SubscriberQos::default(), reliable_reader_qos());

    // Discovery has to cross the forwarder in both directions before either side
    // reports a match, which is the whole point of rewriting the announcements.
    let matched = wait_until(
        || {
            writer.get_publication_matched_status().map(|s| s.current_count() >= 1).unwrap_or(false)
                && reader
                    .get_subscription_matched_status()
                    .map(|s| s.current_count() >= 1)
                    .unwrap_or(false)
        },
        30_000,
    );
    assert!(matched, "writer and reader did not match through the forwarder");

    for value in 1..=SAMPLE_COUNT {
        writer.write(&KeyedDataType::new(1, value * 10), InstanceHandle::NIL).unwrap();
    }

    let received = wait_until(
        || {
            reader
                .read(
                    100,
                    &[SampleStateKind::ANY_SAMPLE_STATE],
                    &[ViewStateKind::ANY_VIEW_STATE],
                    &[InstanceStateKind::ALIVE_INSTANCE_STATE],
                )
                .map(|samples| samples.len() >= SAMPLE_COUNT as usize)
                .unwrap_or(false)
        },
        20_000,
    );
    assert!(received, "reader did not receive the forwarded samples");

    let samples = reader
        .take(
            100,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ALIVE_INSTANCE_STATE],
        )
        .unwrap();
    let values: Vec<i16> = samples.iter().filter_map(|s| s.data().ok().map(|d| d.value)).collect();
    assert_eq!(values, vec![10, 20, 30, 40, 50]);

    writer_participant.delete_contained_entities().unwrap();
    factory.delete_participant(writer_participant).unwrap();
    reader_participant.delete_contained_entities().unwrap();
    factory.delete_participant(reader_participant).unwrap();
    forwarder_a.stop();
    forwarder_b.stop();
}

/// A participant its own forwarder turned away. The far network must not learn
/// of it at all, which takes more than dropping its announcement: it still
/// hears the announcements the forwarder injects, so it addresses the far network
/// directly and the forwarder has to refuse to carry that too.
#[test]
fn a_participant_the_policy_turned_away_never_reaches_the_far_network() {
    let domain_a = next_domain_id();
    let domain_b = next_domain_id();
    let ports_a = ForwarderPorts::for_domain(domain_a);
    let ports_b = ForwarderPorts::for_domain(domain_b);
    let link_address = format!("127.0.0.1:{}", ports_a.link);

    let forwarder_a = Forwarder::start(
        ForwarderConfig::new(
            domain_a as u32,
            ports_a.metatraffic,
            ports_a.user_data,
            LinkRole::Listen(link_address.parse().unwrap()),
        )
        .with_denied_peers(["0.0.0.0/0"]),
    )
    .expect("forwarder A failed to start");
    let forwarder_b =
        start_forwarder(domain_b, &ports_b, LinkRole::Connect(link_address.parse().unwrap()));

    let factory = DomainParticipantFactory::get_instance();
    let writer_participant = factory
        .create_participant(domain_a, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();
    let writer =
        create_datawriter(&writer_participant, PublisherQos::default(), reliable_writer_qos());

    let reader_participant = factory
        .create_participant(domain_b, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();
    let reader =
        create_datareader(&reader_participant, SubscriberQos::default(), reliable_reader_qos());

    let matched = wait_until(
        || {
            writer.get_publication_matched_status().map(|s| s.current_count() >= 1).unwrap_or(false)
                || reader
                    .get_subscription_matched_status()
                    .map(|s| s.current_count() >= 1)
                    .unwrap_or(false)
        },
        10_000,
    );
    assert!(!matched, "a denied participant matched across the forwarder anyway");

    // Matching alone would not prove much: the far side may well ignore an
    // endpoint whose participant it never discovered. What the policy promises
    // is that nothing was put on the link in the first place.
    let stats = forwarder_a.link_stats();
    assert_eq!(
        stats.sent_metatraffic_frames + stats.sent_user_data_frames,
        0,
        "a denied participant still put {} B on the link",
        stats.sent_metatraffic_bytes + stats.sent_user_data_bytes
    );

    writer_participant.delete_contained_entities().unwrap();
    factory.delete_participant(writer_participant).unwrap();
    reader_participant.delete_contained_entities().unwrap();
    factory.delete_participant(reader_participant).unwrap();
    forwarder_a.stop();
    forwarder_b.stop();
}

/// One forwarder serving two peers at once. The writer's network is linked to two
/// others that are not linked to each other, so a sample reaching both readers
/// proves the announcements went out on every link and that each reply found
/// its way back to the link its participant was learned on.
#[test]
fn one_forwarder_forwards_to_two_peers_at_once() {
    let domain_a = next_domain_id();
    let domain_b = next_domain_id();
    let domain_c = next_domain_id();
    let ports_a = ForwarderPorts::for_domain(domain_a);
    let ports_b = ForwarderPorts::for_domain(domain_b);
    let ports_c = ForwarderPorts::for_domain(domain_c);
    let link_to_b: std::net::SocketAddr = format!("127.0.0.1:{}", ports_a.link).parse().unwrap();
    let link_to_c: std::net::SocketAddr =
        format!("127.0.0.1:{}", ports_a.link + 1).parse().unwrap();

    let forwarder_a = Forwarder::start(
        ForwarderConfig::new(
            domain_a as u32,
            ports_a.metatraffic,
            ports_a.user_data,
            LinkRole::Listen(link_to_b),
        )
        .with_peer(LinkRole::Listen(link_to_c)),
    )
    .expect("forwarder A failed to start");
    let forwarder_b = start_forwarder(domain_b, &ports_b, LinkRole::Connect(link_to_b));
    let forwarder_c = start_forwarder(domain_c, &ports_c, LinkRole::Connect(link_to_c));

    let factory = DomainParticipantFactory::get_instance();
    let writer_participant = factory
        .create_participant(domain_a, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();
    let writer =
        create_datawriter(&writer_participant, PublisherQos::default(), reliable_writer_qos());

    let reader_b_participant = factory
        .create_participant(domain_b, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();
    let reader_b =
        create_datareader(&reader_b_participant, SubscriberQos::default(), reliable_reader_qos());
    let reader_c_participant = factory
        .create_participant(domain_c, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();
    let reader_c =
        create_datareader(&reader_c_participant, SubscriberQos::default(), reliable_reader_qos());

    let matched = wait_until(
        || writer.get_publication_matched_status().map(|s| s.current_count() >= 2).unwrap_or(false),
        30_000,
    );
    assert!(matched, "the writer did not match a reader behind each peer forwarder");

    for value in 1..=SAMPLE_COUNT {
        writer.write(&KeyedDataType::new(1, value * 10), InstanceHandle::NIL).unwrap();
    }

    let received = wait_until(
        || {
            [&reader_b, &reader_c].iter().all(|reader| {
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
        20_000,
    );
    assert!(received, "a reader behind one of the two peers did not receive the samples");

    writer_participant.delete_contained_entities().unwrap();
    factory.delete_participant(writer_participant).unwrap();
    reader_b_participant.delete_contained_entities().unwrap();
    factory.delete_participant(reader_b_participant).unwrap();
    reader_c_participant.delete_contained_entities().unwrap();
    factory.delete_participant(reader_c_participant).unwrap();
    forwarder_a.stop();
    forwarder_b.stop();
    forwarder_c.stop();
}

/// One forwarder is restarted under a running writer and reader. Neither
/// participant is touched, so anything that arrives afterwards had to cross a
/// second link that the surviving forwarder accepted on its own.
#[test]
fn traffic_recovers_after_one_forwarder_restarts() {
    let domain_a = next_domain_id();
    let domain_b = next_domain_id();
    let ports_a = ForwarderPorts::for_domain(domain_a);
    let ports_b = ForwarderPorts::for_domain(domain_b);
    let link_address = format!("127.0.0.1:{}", ports_a.link);

    let forwarder_a =
        start_forwarder(domain_a, &ports_a, LinkRole::Listen(link_address.parse().unwrap()));
    let forwarder_b =
        start_forwarder(domain_b, &ports_b, LinkRole::Connect(link_address.parse().unwrap()));

    let factory = DomainParticipantFactory::get_instance();
    let writer_participant = factory
        .create_participant(domain_a, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();
    let writer =
        create_datawriter(&writer_participant, PublisherQos::default(), reliable_writer_qos());

    let reader_participant = factory
        .create_participant(domain_b, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();
    let reader =
        create_datareader(&reader_participant, SubscriberQos::default(), reliable_reader_qos());

    let matched = wait_until(
        || {
            writer.get_publication_matched_status().map(|s| s.current_count() >= 1).unwrap_or(false)
                && reader
                    .get_subscription_matched_status()
                    .map(|s| s.current_count() >= 1)
                    .unwrap_or(false)
        },
        30_000,
    );
    assert!(matched, "writer and reader did not match through the forwarder");

    forwarder_b.stop();
    sleep(StdDuration::from_secs(1));
    let forwarder_b =
        start_forwarder(domain_b, &ports_b, LinkRole::Connect(link_address.parse().unwrap()));

    // Both tables are empty again and refill from the next round of
    // announcements, which is what the samples below have to travel on.
    sleep(StdDuration::from_secs(5));
    for value in 1..=SAMPLE_COUNT {
        writer.write(&KeyedDataType::new(1, value * 100), InstanceHandle::NIL).unwrap();
    }

    let received = wait_until(
        || {
            reader
                .read(
                    100,
                    &[SampleStateKind::ANY_SAMPLE_STATE],
                    &[ViewStateKind::ANY_VIEW_STATE],
                    &[InstanceStateKind::ALIVE_INSTANCE_STATE],
                )
                .map(|samples| samples.len() >= SAMPLE_COUNT as usize)
                .unwrap_or(false)
        },
        30_000,
    );
    assert!(received, "reader did not receive samples after the link was restored");

    writer_participant.delete_contained_entities().unwrap();
    factory.delete_participant(writer_participant).unwrap();
    reader_participant.delete_contained_entities().unwrap();
    factory.delete_participant(reader_participant).unwrap();
    forwarder_a.stop();
    forwarder_b.stop();
}
