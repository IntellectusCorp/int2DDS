//! End-to-end test of the transparent RTPS relay.
//!
//! ```text
//!   [Writer, domain A] ── multicast ──▶ [gateway A] ══ TCP ══ [gateway B] ──▶ [Reader, domain B]
//! ```
//! The two networks are kept apart by running them on different domains, so
//! nothing but the gateway pair can carry discovery or data between them. The
//! writer and the reader are ordinary participants: neither knows a gateway
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
    route_gateway::{LinkRole, RelayConfig, RelayGateway},
    subscription::{
        qos::{DataReaderQos, SubscriberQos},
        sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
    },
};

const SAMPLE_COUNT: i16 = 5;

/// Ports for one gateway, spaced so that concurrently running tests on
/// different domains never collide.
struct GatewayPorts {
    metatraffic: u16,
    user_data: u16,
    link: u16,
}

impl GatewayPorts {
    fn for_domain(domain_id: i32) -> Self {
        let base = 30000 + (domain_id as u16) * 4;
        Self { metatraffic: base, user_data: base + 1, link: base + 2 }
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

#[test]
fn relay_carries_discovery_and_samples_between_two_networks() {
    let domain_a = next_domain_id();
    let domain_b = next_domain_id();
    let ports_a = GatewayPorts::for_domain(domain_a);
    let ports_b = GatewayPorts::for_domain(domain_b);
    let link_address = format!("127.0.0.1:{}", ports_a.link);

    let gateway_a = RelayGateway::start(RelayConfig::new(
        domain_a as u32,
        ports_a.metatraffic,
        ports_a.user_data,
        LinkRole::Listen(link_address.parse().unwrap()),
    ))
    .expect("gateway A failed to start");
    let gateway_b = RelayGateway::start(RelayConfig::new(
        domain_b as u32,
        ports_b.metatraffic,
        ports_b.user_data,
        LinkRole::Connect(link_address.parse().unwrap()),
    ))
    .expect("gateway B failed to start");

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

    // Discovery has to cross the relay in both directions before either side
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
    assert!(matched, "writer and reader did not match through the relay");

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
    assert!(received, "reader did not receive the relayed samples");

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
    gateway_a.stop();
    gateway_b.stop();
}
