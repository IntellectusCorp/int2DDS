//! The peers TLS has authenticated and still holds a connection to.

use std::{
    net::SocketAddr,
    time::{Duration, SystemTime},
};

use rustls::pki_types::CertificateDer;

use crate::rtps::transport::tls::{
    events::TlsDirection,
    identity::{identity_from_cert, IDENTITY_LEN},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TlsPeer {
    pub address: SocketAddr,
    pub direction: TlsDirection,
    pub identity: [u8; IDENTITY_LEN],
    /// `None` when the certificate's fields could not be read, which leaves
    /// the peer listed: the handshake authenticated it either way.
    pub subject: Option<String>,
    pub not_after: Option<SystemTime>,
}

impl TlsPeer {
    pub(crate) fn new(
        address: SocketAddr,
        direction: TlsDirection,
        leaf: &CertificateDer<'_>,
    ) -> Self {
        let parsed = x509_parser::parse_x509_certificate(leaf.as_ref()).ok();
        let subject = parsed.as_ref().map(|(_, cert)| cert.subject().to_string());
        let not_after = parsed
            .as_ref()
            .and_then(|(_, cert)| u64::try_from(cert.validity().not_after.timestamp()).ok())
            .and_then(|seconds| SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(seconds)));
        Self { address, direction, identity: identity_from_cert(leaf), subject, not_after }
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use super::*;
    use crate::rtps::transport::tls::test_pki::{LeafSpec, TestCa};

    fn address() -> SocketAddr {
        SocketAddr::from((Ipv4Addr::LOCALHOST, 7400))
    }

    #[test]
    fn a_peer_shows_who_its_certificate_names_and_until_when() {
        let ca = TestCa::root("ca");
        let leaf = CertificateDer::from(ca.issue(LeafSpec::named("worker-1")).der);

        let peer = TlsPeer::new(address(), TlsDirection::Dialed, &leaf);

        assert_eq!(peer.identity, identity_from_cert(&leaf));
        assert!(peer.subject.unwrap().contains("worker-1"));
        assert!(peer.not_after.unwrap() > SystemTime::now());
    }

    #[test]
    fn a_certificate_it_cannot_read_still_leaves_the_peer_listed() {
        let leaf = CertificateDer::from(vec![0u8; 4]);

        let peer = TlsPeer::new(address(), TlsDirection::Accepted, &leaf);

        assert_eq!(peer.identity, identity_from_cert(&leaf));
        assert_eq!(peer.subject, None);
        assert_eq!(peer.not_after, None);
    }
}
