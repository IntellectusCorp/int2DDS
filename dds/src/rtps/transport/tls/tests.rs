//! Certificate verification exercised through the configs that `TlsConfig`
//! builds, over an in-memory handshake so that no socket timing is involved.

use std::{
    io,
    net::{IpAddr, Ipv4Addr},
    sync::Arc,
};

use rcgen::ExtendedKeyUsagePurpose;
use rustls::{
    pki_types::ServerName, CertificateError, ClientConfig, ClientConnection, Connection,
    ServerConnection,
};

use crate::{
    infrastructure::qos_policy::PropertyQosPolicy,
    rtps::transport::{
        tcp::sync_connection::tls_client,
        tls::{
            config::keys,
            rejection::rejection_reason,
            test_pki::{LeafSpec, TestCa},
            verifier::NameAgnosticServerVerifier,
            TlsConfig,
        },
    },
};

/// Address the client dials, standing in for a discovered locator.
pub(super) const PEER: IpAddr = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1));

/// How each side ended the handshake: the first error it raised, or the one
/// it received as an alert from the other side.
#[derive(Debug)]
pub(super) struct Outcome {
    pub(super) client: Result<(), rustls::Error>,
    pub(super) server: Result<(), rustls::Error>,
}

/// Runs a handshake between the client and server sides that `client` and
/// `server` build, with the client dialing [`PEER`].
pub(super) fn handshake(client: &TlsConfig, server: &TlsConfig) -> io::Result<Outcome> {
    let client = tls_client(client, PEER)?;
    let server = ServerConnection::new(server.build_server_config()?).map_err(io::Error::other)?;
    Ok(drive(client, server))
}

/// Passes records between the two sides until neither has anything left to
/// send, so an alert raised by one side is still delivered to the other.
pub(super) fn drive(client: ClientConnection, server: ServerConnection) -> Outcome {
    let mut sides = [(Connection::Client(client), Ok(())), (Connection::Server(server), Ok(()))];
    loop {
        let mut moved = false;
        for from in 0..2 {
            let mut records = Vec::new();
            while sides[from].0.wants_write() {
                sides[from].0.write_tls(&mut records).unwrap();
            }
            if records.is_empty() {
                continue;
            }
            moved = true;
            let (receiver, result) = &mut sides[1 - from];
            let mut unread = records.as_slice();
            while !unread.is_empty() && result.is_ok() {
                receiver.read_tls(&mut unread).unwrap();
                if let Err(error) = receiver.process_new_packets() {
                    *result = Err(error);
                }
            }
        }
        if !moved {
            break;
        }
    }

    let [client, server] = sides.map(|(connection, result)| match result {
        Ok(()) if connection.is_handshaking() => {
            Err(rustls::Error::General("handshake did not complete".to_string()))
        }
        other => other,
    });
    Outcome { client, server }
}

/// Matches by pattern because several rejections come in a plain form and a
/// form with context, which do not compare equal.
macro_rules! assert_rejected {
    ($result:expr, $reason:pat) => {
        match &$result {
            Err(rustls::Error::InvalidCertificate($reason)) => {}
            other => panic!("expected {}, got {other:?}", stringify!($reason)),
        }
    };
}

#[test]
fn peers_under_one_ca_connect_with_and_without_mutual_tls() {
    let dir = tempfile::tempdir().unwrap();
    let ca = TestCa::root("ca");
    let mut client = ca.issue(LeafSpec::named("client")).config(dir.path(), &[&ca]);
    let mut server = ca.issue(LeafSpec::named("server")).config(dir.path(), &[&ca]);

    let outcome = handshake(&client, &server).unwrap();
    assert!(outcome.client.is_ok() && outcome.server.is_ok(), "{outcome:?}");

    client.verify_peer = false;
    server.verify_peer = false;
    let outcome = handshake(&client, &server).unwrap();
    assert!(outcome.client.is_ok() && outcome.server.is_ok(), "{outcome:?}");
}

#[test]
fn peers_under_different_cas_connect_when_each_trusts_the_other() {
    let dir = tempfile::tempdir().unwrap();
    let ca_a = TestCa::root("ca-a");
    let ca_b = TestCa::root("ca-b");
    let client = ca_a.issue(LeafSpec::named("client")).config(dir.path(), &[&ca_a, &ca_b]);
    let server = ca_b.issue(LeafSpec::named("server")).config(dir.path(), &[&ca_a, &ca_b]);

    let outcome = handshake(&client, &server).unwrap();
    assert!(outcome.client.is_ok() && outcome.server.is_ok(), "{outcome:?}");
}

#[test]
fn a_client_rejects_a_server_whose_ca_it_does_not_trust() {
    let dir = tempfile::tempdir().unwrap();
    let ca_a = TestCa::root("ca-a");
    let ca_b = TestCa::root("ca-b");
    let client = ca_a.issue(LeafSpec::named("client")).config(dir.path(), &[&ca_a]);
    let server = ca_b.issue(LeafSpec::named("server")).config(dir.path(), &[&ca_a, &ca_b]);

    let outcome = handshake(&client, &server).unwrap();
    assert_rejected!(outcome.client, CertificateError::UnknownIssuer);
    assert!(outcome.server.is_err(), "{outcome:?}");
}

#[test]
fn a_server_rejects_a_client_whose_ca_it_does_not_trust() {
    let dir = tempfile::tempdir().unwrap();
    let ca_a = TestCa::root("ca-a");
    let ca_b = TestCa::root("ca-b");
    let client = ca_a.issue(LeafSpec::named("client")).config(dir.path(), &[&ca_a, &ca_b]);
    let server = ca_b.issue(LeafSpec::named("server")).config(dir.path(), &[&ca_b]);

    let outcome = handshake(&client, &server).unwrap();
    assert_rejected!(outcome.server, CertificateError::UnknownIssuer);
    assert!(outcome.client.is_err(), "{outcome:?}");
}

#[test]
fn a_leaf_under_an_intermediate_chains_to_a_trusted_root() {
    let dir = tempfile::tempdir().unwrap();
    let root = TestCa::root("root");
    let intermediate = root.intermediate("intermediate");
    let client = root.issue(LeafSpec::named("client")).config(dir.path(), &[&root]);
    let server = intermediate.issue(LeafSpec::named("server")).config(dir.path(), &[&root]);

    let outcome = handshake(&client, &server).unwrap();
    assert!(outcome.client.is_ok() && outcome.server.is_ok(), "{outcome:?}");
}

#[test]
fn a_leaf_that_leaves_out_its_intermediate_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let root = TestCa::root("root");
    let intermediate = root.intermediate("intermediate");
    let client = root.issue(LeafSpec::named("client")).config(dir.path(), &[&root]);
    let server = intermediate
        .issue(LeafSpec { with_intermediates: false, ..LeafSpec::named("server") })
        .config(dir.path(), &[&root]);

    let outcome = handshake(&client, &server).unwrap();
    assert_rejected!(outcome.client, CertificateError::UnknownIssuer);
}

#[test]
fn trusting_only_an_intermediate_accepts_the_leaves_it_signs() {
    let dir = tempfile::tempdir().unwrap();
    let root = TestCa::root("root");
    let intermediate = root.intermediate("intermediate");
    let client = intermediate.issue(LeafSpec::named("client")).config(dir.path(), &[&intermediate]);
    let server = intermediate.issue(LeafSpec::named("server")).config(dir.path(), &[&intermediate]);

    let outcome = handshake(&client, &server).unwrap();
    assert!(outcome.client.is_ok() && outcome.server.is_ok(), "{outcome:?}");
}

#[test]
fn an_expired_certificate_is_rejected_on_either_side() {
    let dir = tempfile::tempdir().unwrap();
    let ca = TestCa::root("ca");
    let expired = || LeafSpec { valid_years: (2000, 2001), ..LeafSpec::named("expired") };
    let valid_client = ca.issue(LeafSpec::named("client")).config(dir.path(), &[&ca]);
    let valid_server = ca.issue(LeafSpec::named("server")).config(dir.path(), &[&ca]);
    let expired = ca.issue(expired()).config(dir.path(), &[&ca]);

    let outcome = handshake(&valid_client, &expired).unwrap();
    assert_rejected!(
        outcome.client,
        CertificateError::Expired | CertificateError::ExpiredContext { .. }
    );

    let outcome = handshake(&expired, &valid_server).unwrap();
    assert_rejected!(
        outcome.server,
        CertificateError::Expired | CertificateError::ExpiredContext { .. }
    );
}

#[test]
fn a_certificate_that_is_not_valid_yet_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let ca = TestCa::root("ca");
    let client = ca.issue(LeafSpec::named("client")).config(dir.path(), &[&ca]);
    let server = ca
        .issue(LeafSpec { valid_years: (2998, 2999), ..LeafSpec::named("server") })
        .config(dir.path(), &[&ca]);

    let outcome = handshake(&client, &server).unwrap();
    assert_rejected!(
        outcome.client,
        CertificateError::NotValidYet | CertificateError::NotValidYetContext { .. }
    );
}

#[test]
fn without_a_server_name_any_name_on_a_trusted_certificate_is_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let ca = TestCa::root("ca");
    let client = ca.issue(LeafSpec::named("client")).config(dir.path(), &[&ca]);
    let server = ca
        .issue(LeafSpec {
            sans: vec!["unrelated.example".to_string()],
            ..LeafSpec::named("server")
        })
        .config(dir.path(), &[&ca]);

    let outcome = handshake(&client, &server).unwrap();
    assert!(outcome.client.is_ok() && outcome.server.is_ok(), "{outcome:?}");
}

#[test]
fn a_configured_server_name_must_be_carried_by_the_server_certificate() {
    let dir = tempfile::tempdir().unwrap();
    let ca = TestCa::root("ca");
    let mut client = ca.issue(LeafSpec::named("client")).config(dir.path(), &[&ca]);
    let server = ca.issue(LeafSpec::named("server")).config(dir.path(), &[&ca]);

    client.server_name = Some("server.int2dds.test".to_string());
    let outcome = handshake(&client, &server).unwrap();
    assert!(outcome.client.is_ok() && outcome.server.is_ok(), "{outcome:?}");

    client.server_name = Some("other.int2dds.test".to_string());
    let outcome = handshake(&client, &server).unwrap();
    assert_rejected!(
        outcome.client,
        CertificateError::NotValidForName | CertificateError::NotValidForNameContext { .. }
    );
}

#[test]
fn a_server_certificate_without_server_auth_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let ca = TestCa::root("ca");
    let client = ca.issue(LeafSpec::named("client")).config(dir.path(), &[&ca]);
    let server = ca
        .issue(LeafSpec {
            usages: vec![ExtendedKeyUsagePurpose::ClientAuth],
            ..LeafSpec::named("server")
        })
        .config(dir.path(), &[&ca]);

    let outcome = handshake(&client, &server).unwrap();
    assert_rejected!(
        outcome.client,
        CertificateError::InvalidPurpose | CertificateError::InvalidPurposeContext { .. }
    );
}

#[test]
fn a_client_certificate_without_client_auth_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let ca = TestCa::root("ca");
    let client = ca
        .issue(LeafSpec {
            usages: vec![ExtendedKeyUsagePurpose::ServerAuth],
            ..LeafSpec::named("client")
        })
        .config(dir.path(), &[&ca]);
    let server = ca.issue(LeafSpec::named("server")).config(dir.path(), &[&ca]);

    let outcome = handshake(&client, &server).unwrap();
    assert_rejected!(
        outcome.server,
        CertificateError::InvalidPurpose | CertificateError::InvalidPurposeContext { .. }
    );
}

#[test]
fn a_key_that_does_not_match_its_certificate_fails_before_any_handshake() {
    let dir = tempfile::tempdir().unwrap();
    let ca = TestCa::root("ca");
    let mut mismatched = ca.issue(LeafSpec::named("client"));
    mismatched.key_pem = ca.issue(LeafSpec::named("other")).key_pem;
    let client = mismatched.config(dir.path(), &[&ca]);
    let server = ca.issue(LeafSpec::named("server")).config(dir.path(), &[&ca]);

    let client_error = handshake(&client, &server).unwrap_err();
    assert!(client_error.to_string().contains("KeyMismatch"), "{client_error}");
    let server_error = handshake(&server, &client).unwrap_err();
    assert!(server_error.to_string().contains("KeyMismatch"), "{server_error}");
}

/// A client that trusts the same CAs as `trusting` but presents no certificate.
fn anonymous_client(trusting: &TlsConfig) -> ClientConnection {
    let verifier = NameAgnosticServerVerifier::new(trusting.load_root_store().unwrap()).unwrap();
    let config = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();
    ClientConnection::new(Arc::new(config), ServerName::from(PEER)).unwrap()
}

fn server_side(config: &TlsConfig) -> ServerConnection {
    ServerConnection::new(config.build_server_config().unwrap()).unwrap()
}

#[test]
fn a_client_without_a_certificate_is_rejected_under_mutual_tls() {
    let dir = tempfile::tempdir().unwrap();
    let ca = TestCa::root("ca");
    let server = ca.issue(LeafSpec::named("server")).config(dir.path(), &[&ca]);

    let outcome = drive(anonymous_client(&server), server_side(&server));
    assert!(matches!(outcome.server, Err(rustls::Error::NoCertificatesPresented)), "{outcome:?}");
}

#[test]
fn a_client_without_a_certificate_is_accepted_when_mutual_tls_is_off() {
    let dir = tempfile::tempdir().unwrap();
    let ca = TestCa::root("ca");
    let mut server = ca.issue(LeafSpec::named("server")).config(dir.path(), &[&ca]);
    server.verify_peer = false;

    let outcome = drive(anonymous_client(&server), server_side(&server));
    assert!(outcome.client.is_ok() && outcome.server.is_ok(), "{outcome:?}");
}

#[test]
fn an_untrusted_client_certificate_is_not_checked_when_mutual_tls_is_off() {
    let dir = tempfile::tempdir().unwrap();
    let trusted = TestCa::root("trusted");
    let untrusted = TestCa::root("untrusted");
    let client = untrusted.issue(LeafSpec::named("client")).config(dir.path(), &[&trusted]);
    let mut server = trusted.issue(LeafSpec::named("server")).config(dir.path(), &[&trusted]);
    server.verify_peer = false;

    let outcome = handshake(&client, &server).unwrap();
    assert!(outcome.client.is_ok() && outcome.server.is_ok(), "{outcome:?}");
}

#[test]
fn a_config_read_from_properties_requires_client_certificates_by_default() {
    let dir = tempfile::tempdir().unwrap();
    let ca = TestCa::root("ca");
    let files = ca.issue(LeafSpec::named("server")).config(dir.path(), &[&ca]);
    let mut property = PropertyQosPolicy::default();
    property.add_property(keys::CA_FILE, files.ca_file.to_str().unwrap(), false);
    property.add_property(keys::CERT_FILE, files.cert_file.to_str().unwrap(), false);
    property.add_property(keys::KEY_FILE, files.key_file.to_str().unwrap(), false);
    let server = TlsConfig::from_property(&property).unwrap().unwrap();

    let outcome = drive(anonymous_client(&server), server_side(&server));
    assert!(matches!(outcome.server, Err(rustls::Error::NoCertificatesPresented)), "{outcome:?}");
}

/// The reason for a handshake result once it is wrapped the way the TLS
/// stream hands it to the transport.
fn reason_for(result: Result<(), rustls::Error>) -> Option<&'static str> {
    rejection_reason(&io::Error::new(io::ErrorKind::InvalidData, result.unwrap_err()))
}

#[test]
fn rejection_reasons_name_what_the_rejecting_side_found_wrong() {
    let dir = tempfile::tempdir().unwrap();
    let ca = TestCa::root("ca");
    let stranger = TestCa::root("stranger");
    let client = ca.issue(LeafSpec::named("client")).config(dir.path(), &[&ca]);
    let mut named_client = client.clone();
    named_client.server_name = Some("other.int2dds.test".to_string());
    let issue = |spec: LeafSpec| ca.issue(spec).config(dir.path(), &[&ca]);

    let untrusted = stranger.issue(LeafSpec::named("untrusted")).config(dir.path(), &[&ca]);
    let expired = issue(LeafSpec { valid_years: (2000, 2001), ..LeafSpec::named("expired") });
    let future = issue(LeafSpec { valid_years: (2998, 2999), ..LeafSpec::named("future") });
    let client_only = issue(LeafSpec {
        usages: vec![ExtendedKeyUsagePurpose::ClientAuth],
        ..LeafSpec::named("client-only")
    });
    let server = issue(LeafSpec::named("server"));

    let cases = [
        (&client, &untrusted, "certificate is not signed by a trusted CA"),
        (&client, &expired, "certificate has expired"),
        (&client, &future, "certificate is not valid yet"),
        (&named_client, &server, "certificate does not carry the configured server name"),
        (&client, &client_only, "certificate is not issued for this role"),
    ];
    for (client, server, expected) in cases {
        let outcome = handshake(client, server).unwrap();
        assert_eq!(reason_for(outcome.client), Some(expected));
    }

    let outcome = drive(anonymous_client(&server), server_side(&server));
    assert_eq!(reason_for(outcome.server), Some("peer presented no certificate"));
}

#[test]
fn the_rejected_side_learns_that_its_certificate_was_refused() {
    let dir = tempfile::tempdir().unwrap();
    let ca = TestCa::root("ca");
    let stranger = TestCa::root("stranger");
    let client = stranger.issue(LeafSpec::named("client")).config(dir.path(), &[&ca, &stranger]);
    let server = ca.issue(LeafSpec::named("server")).config(dir.path(), &[&ca]);

    let outcome = handshake(&client, &server).unwrap();
    assert_eq!(reason_for(outcome.client), Some("peer rejected this participant's certificate"));
}

#[test]
fn failures_unrelated_to_certificates_have_no_rejection_reason() {
    let timeout = io::Error::new(io::ErrorKind::TimedOut, "TLS handshake timed out");
    let protocol = io::Error::new(
        io::ErrorKind::InvalidData,
        rustls::Error::General("unexpected message".to_string()),
    );

    assert_eq!(rejection_reason(&timeout), None);
    assert_eq!(rejection_reason(&protocol), None);
}
