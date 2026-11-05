//! Reloading the TLS policy through the public API while two participants
//! exchange samples over mutual TLS.

use std::{
    fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration as StdDuration, Instant},
};

use crate::common::*;
use int2dds::{
    common::instance_handle::InstanceHandle,
    dcps::{
        core::{error::DdsError, time::Duration},
        domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
        infrastructure::{
            qos_policy::{PropertyQosPolicy, ReliabilityQosPolicy, ReliabilityQosPolicyKind},
            status::StatusMask,
        },
        publication::{
            data_writer::DataWriter,
            qos::{DataWriterQos, PublisherQos},
        },
        subscription::{
            data_reader::DataReader,
            qos::{DataReaderQos, SubscriberQos},
            sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
        },
    },
};
use rcgen::{
    date_time_ymd, BasicConstraints, Certificate, CertificateParams,
    CertificateRevocationListParams, DnType, ExtendedKeyUsagePurpose, IsCa, KeyIdMethod, KeyPair,
    RevokedCertParams, SerialNumber,
};

const WRITER_SERIAL: u64 = 2;
const READER_SERIAL: u64 = 3;

pub(super) struct Ca {
    cert: Certificate,
    key: KeyPair,
}

impl Ca {
    pub(super) fn new() -> Self {
        let key = KeyPair::generate().unwrap();
        let mut params = CertificateParams::new(Vec::<String>::new()).unwrap();
        params.distinguished_name.push(DnType::CommonName, "reload-ca");
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.serial_number = Some(SerialNumber::from(1u64));
        let cert = params.self_signed(&key).unwrap();
        Self { cert, key }
    }

    fn crl(&self, revoked: &[u64]) -> String {
        let revoked_certs = revoked
            .iter()
            .map(|serial| RevokedCertParams {
                serial_number: SerialNumber::from(*serial),
                revocation_time: date_time_ymd(2001, 1, 1),
                reason_code: None,
                invalidity_date: None,
            })
            .collect();
        CertificateRevocationListParams {
            this_update: date_time_ymd(2001, 1, 1),
            next_update: date_time_ymd(2999, 1, 1),
            crl_number: SerialNumber::from(10u64 + revoked.len() as u64),
            issuing_distribution_point: None,
            revoked_certs,
            key_identifier_method: KeyIdMethod::Sha256,
        }
        .signed_by(&self.cert, &self.key)
        .unwrap()
        .pem()
        .unwrap()
    }
}

/// Writes one node's certificate, key, trust and CRL files and returns the QoS
/// pointing at them with the path of its CRL.
pub(super) fn tls_node(
    dir: &Path,
    ca: &Ca,
    name: &str,
    serial: u64,
    bind_port: u16,
    peer_port: u16,
) -> (DomainParticipantQos, PathBuf) {
    let mut params = CertificateParams::new(vec![format!("{name}.int2dds.test")]).unwrap();
    params.distinguished_name.push(DnType::CommonName, name);
    params.extended_key_usages =
        vec![ExtendedKeyUsagePurpose::ServerAuth, ExtendedKeyUsagePurpose::ClientAuth];
    params.serial_number = Some(SerialNumber::from(serial));
    let key = KeyPair::generate().unwrap();
    let cert = params.signed_by(&key, &ca.cert, &ca.key).unwrap();

    let path = |suffix: &str| dir.join(format!("{name}-{suffix}.pem"));
    let (ca_file, cert_file, key_file, crl_file) =
        (path("ca"), path("cert"), path("key"), path("crl"));
    fs::write(&ca_file, ca.cert.pem()).unwrap();
    fs::write(&cert_file, cert.pem()).unwrap();
    fs::write(&key_file, key.serialize_pem()).unwrap();
    fs::write(&crl_file, ca.crl(&[])).unwrap();

    let mut property = PropertyQosPolicy::default();
    property.add_property("int2dds.transport", "tcp", false);
    property.add_property("int2dds.initial_peers", format!("127.0.0.1:{peer_port}"), false);
    property.add_property("int2dds.tls.ca_file", ca_file.to_str().unwrap(), false);
    property.add_property("int2dds.tls.cert_file", cert_file.to_str().unwrap(), false);
    property.add_property("int2dds.tls.key_file", key_file.to_str().unwrap(), false);
    property.add_property("int2dds.tls.crl_file", crl_file.to_str().unwrap(), false);
    property.set_tcp_bind_port(bind_port);
    (DomainParticipantQos { property, ..Default::default() }, crl_file)
}

pub(super) fn free_tcp_ports() -> (u16, u16) {
    let first = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let second = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    (first.local_addr().unwrap().port(), second.local_addr().unwrap().port())
}

fn take_values(reader: &DataReader<KeyedDataType>) -> Vec<i16> {
    reader
        .take(
            100,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        )
        .unwrap_or_default()
        .iter()
        .filter_map(|sample| sample.data().ok().map(|data| data.value))
        .collect()
}

/// Keeps writing `value` until the reader sees it or `wait` runs out.
pub(super) fn reaches_reader(
    writer: &DataWriter<KeyedDataType>,
    reader: &DataReader<KeyedDataType>,
    value: i16,
    wait: StdDuration,
) -> bool {
    let deadline = Instant::now() + wait;
    while Instant::now() < deadline {
        let _ = writer.write(&KeyedDataType::new(1, value), InstanceHandle::NIL);
        if take_values(reader).contains(&value) {
            return true;
        }
        thread::sleep(StdDuration::from_millis(20));
    }
    false
}

#[derive(Clone, Copy)]
enum Revoker {
    Writer,
    Reader,
}

/// Samples flow, survive a broken update, and stop once `revoker` revokes the
/// other side. Which side reloads decides which half of the transport has to
/// close: the reader's accepted connection or the writer's dialed one.
fn samples_stop_after_a_runtime_revocation(revoker: Revoker) {
    let dir = tempfile::tempdir().unwrap();
    let ca = Ca::new();
    let (writer_port, reader_port) = free_tcp_ports();
    let (writer_qos, writer_crl) =
        tls_node(dir.path(), &ca, "writer", WRITER_SERIAL, writer_port, reader_port);
    let (reader_qos, reader_crl) =
        tls_node(dir.path(), &ca, "reader", READER_SERIAL, reader_port, writer_port);

    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let writer_participant =
        factory.create_participant(domain_id, writer_qos, None, StatusMask::default()).unwrap();
    let reader_participant =
        factory.create_participant(domain_id, reader_qos, None, StatusMask::default()).unwrap();

    let best_effort = ReliabilityQosPolicy {
        kind: ReliabilityQosPolicyKind::BestEffort,
        max_blocking_time: Duration { sec: 0, nanosec: 0 },
    };
    let data_writer = create_datawriter(
        &writer_participant,
        PublisherQos::default(),
        DataWriterQos { reliability: best_effort, ..Default::default() },
    );
    let data_reader = create_datareader(
        &reader_participant,
        SubscriberQos::default(),
        DataReaderQos { reliability: best_effort, ..Default::default() },
    );

    let (reloading, crl_file, revoked_serial) = match revoker {
        Revoker::Writer => (&writer_participant, &writer_crl, READER_SERIAL),
        Revoker::Reader => (&reader_participant, &reader_crl, WRITER_SERIAL),
    };

    let wait = StdDuration::from_secs(30);
    let flowed_at_start = reaches_reader(&data_writer, &data_reader, 1, wait);

    fs::write(crl_file, "not a CRL").unwrap();
    let broken_update = reloading.reload_tls_policy();
    let flowed_after_broken_update = reaches_reader(&data_writer, &data_reader, 2, wait);

    fs::write(crl_file, ca.crl(&[revoked_serial])).unwrap();
    let revocation = reloading.reload_tls_policy();
    // The listener applies a reload on its next poll, and frames already on
    // their way still arrive, so what is left is cleared before measuring.
    thread::sleep(StdDuration::from_millis(500));
    take_values(&data_reader);
    let flowed_after_revocation =
        reaches_reader(&data_writer, &data_reader, 3, StdDuration::from_secs(3));

    writer_participant.delete_contained_entities().unwrap();
    reader_participant.delete_contained_entities().unwrap();
    factory.delete_participant(writer_participant).unwrap();
    factory.delete_participant(reader_participant).unwrap();

    assert!(flowed_at_start, "samples never flowed over mutual TLS");
    assert!(broken_update.is_err(), "a broken CRL must fail the reload");
    assert!(flowed_after_broken_update, "a failed reload must leave traffic alone");
    assert_eq!(revocation, Ok(()));
    assert!(!flowed_after_revocation, "a revoked peer must stop exchanging samples");
}

#[test]
fn a_reader_that_revokes_its_writer_at_runtime_stops_receiving() {
    samples_stop_after_a_runtime_revocation(Revoker::Reader);
}

#[test]
fn a_writer_that_revokes_its_reader_at_runtime_stops_sending() {
    samples_stop_after_a_runtime_revocation(Revoker::Writer);
}

#[test]
fn reloading_without_tls_is_a_precondition_error() {
    let factory = DomainParticipantFactory::get_instance();
    let mut property = PropertyQosPolicy::default();
    property.add_property("int2dds.transport", "tcp", false);
    property.add_property("int2dds.initial_peers", "127.0.0.1:1", false);
    let qos = DomainParticipantQos { property, ..Default::default() };
    let participant =
        factory.create_participant(next_domain_id(), qos, None, StatusMask::default()).unwrap();

    let result = participant.reload_tls_policy();

    participant.delete_contained_entities().unwrap();
    factory.delete_participant(participant).unwrap();
    assert_eq!(result, Err(DdsError::PreconditionNotMet));
}
