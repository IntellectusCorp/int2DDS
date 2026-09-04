//! Same-host traffic must leave as one datagram per sample, whatever the number
//! of interface addresses the peer advertised.
//!
//! Every address a same-host peer announces belongs to this host, so the kernel
//! routes each copy through the loopback device no matter which of them is
//! picked. `/proc/net/dev` therefore counts the copies: one datagram per sample
//! means the narrowing held, one per interface address means it did not. On a
//! host with a single usable address the ratio is one either way and the test
//! passes without proving anything - it can only fail where duplication is
//! possible.

#![cfg(target_os = "linux")]

mod common;

use common::*;
use int2dds::{
    common::instance_handle::InstanceHandle,
    dcps::{
        core::time::Duration,
        domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
        infrastructure::qos_policy::{Property, PropertyQosPolicy},
        infrastructure::{
            qos_policy::{
                HistoryQosPolicy, HistoryQosPolicyKind, ReliabilityQosPolicy,
                ReliabilityQosPolicyKind,
            },
            status::StatusMask,
        },
        publication::qos::{DataWriterQos, PublisherQos},
        subscription::qos::{DataReaderQos, SubscriberQos},
    },
};

const SAMPLES: i16 = 1000;

/// The loopback counter is machine-wide, so two measurements must not overlap.
static MEASUREMENT: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Datagrams the loopback device has transmitted so far. The `lo` row of
/// `/proc/net/dev` holds eight receive counters before the eight transmit ones,
/// so the transmitted packet count is the tenth field after the name.
fn loopback_tx_packets() -> u64 {
    let stats = std::fs::read_to_string("/proc/net/dev").expect("read /proc/net/dev");
    for line in stats.lines() {
        let Some((name, counters)) = line.split_once(':') else {
            continue;
        };
        if name.trim() != "lo" {
            continue;
        }
        return counters
            .split_ascii_whitespace()
            .nth(9)
            .and_then(|field| field.parse().ok())
            .expect("loopback transmit packet counter");
    }
    panic!("no loopback row in /proc/net/dev");
}

/// Distinct addresses this host has dialled to reach `listen_ports`. Only the
/// dialling side of a connection carries a listen port as its remote port, so
/// one entry per address the peer was actually reached at. `/proc/net/tcp`
/// stores each address as a host-order hex word.
fn dialled_addresses(listen_ports: &[u16]) -> std::collections::BTreeSet<std::net::Ipv4Addr> {
    let table = std::fs::read_to_string("/proc/net/tcp").expect("read /proc/net/tcp");
    let mut addresses = std::collections::BTreeSet::new();

    for line in table.lines().skip(1) {
        let fields: Vec<&str> = line.split_ascii_whitespace().collect();
        let (Some(remote), Some(state)) = (fields.get(2), fields.get(3)) else {
            continue;
        };
        if *state != "01" {
            continue;
        }
        let Some((address, port)) = remote.rsplit_once(':') else {
            continue;
        };
        let (Ok(address), Ok(port)) =
            (u32::from_str_radix(address, 16), u16::from_str_radix(port, 16))
        else {
            continue;
        };
        if listen_ports.contains(&port) {
            addresses.insert(std::net::Ipv4Addr::from(address.to_le_bytes()));
        }
    }
    addresses
}

/// Holds `INT2DDS_USE_LOOPBACK_INTERFACE` for as long as it is in scope and puts
/// back whatever was there before. The variable is process-wide, so a test that
/// panicked while it was set would otherwise leave it behind for the rest.
struct LoopbackInterface(bool);

impl LoopbackInterface {
    fn enabled() -> Self {
        let restore = int2dds::common::env::get_use_loopback_interface();
        int2dds::common::env::set_use_loopback_interface(true);
        Self(restore)
    }
}

impl Drop for LoopbackInterface {
    fn drop(&mut self) {
        int2dds::common::env::set_use_loopback_interface(self.0);
    }
}

/// Write `SAMPLES` samples between two participants of this host and report how
/// many loopback datagrams that took per sample.
fn loopback_datagrams_per_sample(
    reliability: ReliabilityQosPolicy,
    writer_participant_qos: DomainParticipantQos,
    reader_participant_qos: DomainParticipantQos,
) -> f64 {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();

    let writer_participant = factory
        .create_participant(domain_id, writer_participant_qos, None, StatusMask::default())
        .unwrap();
    let reader_participant = factory
        .create_participant(domain_id, reader_participant_qos, None, StatusMask::default())
        .unwrap();

    let writer_qos = DataWriterQos {
        history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepLast(1), ..Default::default() },
        reliability: reliability.clone(),
        ..Default::default()
    };
    let reader_qos = DataReaderQos { reliability, ..Default::default() };

    let data_writer = create_datawriter(&writer_participant, PublisherQos::default(), writer_qos);
    let data_reader = create_datareader(&reader_participant, SubscriberQos::default(), reader_qos);

    wait_for_reader_status(
        &data_reader,
        StatusMask::SUBSCRIPTION_MATCHED,
        Duration::from_seconds(30),
    )
    .unwrap();
    wait_for_writer_status(
        &data_writer,
        StatusMask::PUBLICATION_MATCHED,
        Duration::from_seconds(30),
    )
    .unwrap();

    let before = loopback_tx_packets();
    for value in 0..SAMPLES {
        data_writer.write(&KeyedDataType::new(1, value), InstanceHandle::NIL).unwrap();
    }
    let sent = loopback_tx_packets() - before;

    writer_participant.delete_contained_entities().unwrap();
    reader_participant.delete_contained_entities().unwrap();
    factory.delete_participant(writer_participant).unwrap();
    factory.delete_participant(reader_participant).unwrap();

    sent as f64 / SAMPLES as f64
}

fn assert_one_datagram_per_sample(per_sample: f64, path: &str) {
    assert!(
        per_sample > 0.5,
        "{}: only {:.2} loopback datagrams per sample - the samples never reached the wire, \
         so this measurement proves nothing",
        path,
        per_sample
    );
    assert!(
        per_sample < 1.5,
        "{}: {:.2} loopback datagrams per sample - a same-host peer is still being written to \
         once per interface address",
        path,
        per_sample
    );
}

/// Best effort matches through reader locators, one per announced address.
#[test]
fn same_host_best_effort_writer_sends_one_datagram_per_sample() {
    let _measuring = MEASUREMENT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    let per_sample = loopback_datagrams_per_sample(
        ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::BestEffort,
            max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
        },
        DomainParticipantQos::default(),
        DomainParticipantQos::default(),
    );

    println!("best effort: {:.3} loopback datagrams per sample", per_sample);
    assert_one_datagram_per_sample(per_sample, "best effort");
}

/// Reliable matches through a reader proxy that carries the whole list at once.
#[test]
fn same_host_reliable_writer_sends_one_datagram_per_sample() {
    let _measuring = MEASUREMENT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    let per_sample = loopback_datagrams_per_sample(
        ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::Reliable,
            max_blocking_time: Duration { sec: 1, nanosec: 0 },
        },
        DomainParticipantQos::default(),
        DomainParticipantQos::default(),
    );

    println!("reliable: {:.3} loopback datagrams per sample", per_sample);
    assert_one_datagram_per_sample(per_sample, "reliable");
}

/// TCP carries a stream, so segments do not map to samples - what duplication
/// costs there is a whole extra connection per interface address. Discovery
/// stays on UDP multicast (hybrid) because pure TCP has none.
#[test]
fn same_host_hybrid_peer_is_dialled_once() {
    let _measuring = MEASUREMENT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    const WRITER_PORT: u16 = 17410;
    const READER_PORT: u16 = 17411;

    // Two participants in one process cannot share a TCP listen port.
    let tcp_participant = |bind_port: u16| DomainParticipantQos {
        property: PropertyQosPolicy {
            value: vec![
                Property {
                    name: "int2dds.transport".to_string(),
                    value: "hybrid".to_string(),
                    propagate: false,
                },
                Property {
                    name: "int2dds.transport.TCPv4.bind_port".to_string(),
                    value: bind_port.to_string(),
                    propagate: false,
                },
            ],
            ..Default::default()
        },
        ..Default::default()
    };

    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let writer_participant = factory
        .create_participant(domain_id, tcp_participant(WRITER_PORT), None, StatusMask::default())
        .unwrap();
    let reader_participant = factory
        .create_participant(domain_id, tcp_participant(READER_PORT), None, StatusMask::default())
        .unwrap();

    let reliable = ReliabilityQosPolicy {
        kind: ReliabilityQosPolicyKind::Reliable,
        max_blocking_time: Duration { sec: 1, nanosec: 0 },
    };
    let writer_qos = DataWriterQos {
        history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepLast(1), ..Default::default() },
        reliability: reliable.clone(),
        ..Default::default()
    };
    let reader_qos = DataReaderQos { reliability: reliable, ..Default::default() };

    let data_writer = create_datawriter(&writer_participant, PublisherQos::default(), writer_qos);
    let data_reader = create_datareader(&reader_participant, SubscriberQos::default(), reader_qos);

    wait_for_reader_status(
        &data_reader,
        StatusMask::SUBSCRIPTION_MATCHED,
        Duration::from_seconds(30),
    )
    .unwrap();
    wait_for_writer_status(
        &data_writer,
        StatusMask::PUBLICATION_MATCHED,
        Duration::from_seconds(30),
    )
    .unwrap();

    for value in 0..SAMPLES {
        data_writer.write(&KeyedDataType::new(1, value), InstanceHandle::NIL).unwrap();
    }
    wait_for_reader_status(&data_reader, StatusMask::DATA_AVAILABLE, Duration::from_seconds(10))
        .unwrap();

    let dialled = dialled_addresses(&[WRITER_PORT, READER_PORT]);
    println!("tcp: peer reached at {:?}", dialled);

    writer_participant.delete_contained_entities().unwrap();
    reader_participant.delete_contained_entities().unwrap();
    factory.delete_participant(writer_participant).unwrap();
    factory.delete_participant(reader_participant).unwrap();

    assert!(!dialled.is_empty(), "tcp: nothing was dialled, so this measurement proves nothing");
    assert_eq!(
        dialled.len(),
        1,
        "tcp: the same-host peer was dialled at {:?} - one connection per interface address \
         instead of one connection",
        dialled
    );
}

/// Pure TCP has no multicast, so the peer is named outright and loopback has to
/// be a working interface for `127.0.0.1` to be listened on, announced and then
/// dialled. Without it the initial peer answers nowhere and nothing is reached.
#[test]
fn same_host_tcp_peer_is_dialled_once() {
    let _measuring = MEASUREMENT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    const WRITER_PORT: u16 = 17420;
    const READER_PORT: u16 = 17421;

    let tcp_participant = |bind_port: u16, peer_port: u16| {
        let mut property = PropertyQosPolicy::default();
        property.add_property("int2dds.transport", "tcp", false);
        property.add_property("int2dds.initial_peers", format!("127.0.0.1:{}", peer_port), false);
        property.set_tcp_bind_port(bind_port);
        DomainParticipantQos { property, ..Default::default() }
    };

    // Read while a participant builds its interface list, so the setting only
    // has to hold until both exist.
    let loopback_interface = LoopbackInterface::enabled();
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let writer_participant = factory
        .create_participant(
            domain_id,
            tcp_participant(WRITER_PORT, READER_PORT),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let reader_participant = factory
        .create_participant(
            domain_id,
            tcp_participant(READER_PORT, WRITER_PORT),
            None,
            StatusMask::default(),
        )
        .unwrap();
    drop(loopback_interface);

    let reliable = ReliabilityQosPolicy {
        kind: ReliabilityQosPolicyKind::Reliable,
        max_blocking_time: Duration { sec: 1, nanosec: 0 },
    };
    let writer_qos = DataWriterQos {
        history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepLast(1), ..Default::default() },
        reliability: reliable.clone(),
        ..Default::default()
    };
    let reader_qos = DataReaderQos { reliability: reliable, ..Default::default() };

    let data_writer = create_datawriter(&writer_participant, PublisherQos::default(), writer_qos);
    let data_reader = create_datareader(&reader_participant, SubscriberQos::default(), reader_qos);

    wait_for_reader_status(
        &data_reader,
        StatusMask::SUBSCRIPTION_MATCHED,
        Duration::from_seconds(30),
    )
    .unwrap();
    wait_for_writer_status(
        &data_writer,
        StatusMask::PUBLICATION_MATCHED,
        Duration::from_seconds(30),
    )
    .unwrap();

    for value in 0..SAMPLES {
        data_writer.write(&KeyedDataType::new(1, value), InstanceHandle::NIL).unwrap();
    }
    wait_for_reader_status(&data_reader, StatusMask::DATA_AVAILABLE, Duration::from_seconds(10))
        .unwrap();

    let dialled = dialled_addresses(&[WRITER_PORT, READER_PORT]);
    println!("tcp only: peer reached at {:?}", dialled);

    writer_participant.delete_contained_entities().unwrap();
    reader_participant.delete_contained_entities().unwrap();
    factory.delete_participant(writer_participant).unwrap();
    factory.delete_participant(reader_participant).unwrap();

    assert!(
        !dialled.is_empty(),
        "tcp only: nothing was dialled, so this measurement proves nothing"
    );
    assert_eq!(
        dialled.len(),
        1,
        "tcp only: the same-host peer was dialled at {:?} - one connection per interface address \
         instead of one connection",
        dialled
    );
}
