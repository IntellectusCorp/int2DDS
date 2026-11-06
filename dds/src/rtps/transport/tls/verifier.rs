//! Peer certificate verification on top of the standard chain checks.
//!
//! A participant reaches its peers through discovered locators, not names, so
//! there is often no name a peer's certificate could be expected to carry;
//! identity is established by binding the GUID prefix to the certificate
//! instead. Allow and deny lists then narrow which validly signed peers may
//! connect, in both directions.

use std::sync::Arc;

use rustls::{
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    client::WebPkiServerVerifier,
    pki_types::{CertificateDer, ServerName, UnixTime},
    server::danger::{ClientCertVerified, ClientCertVerifier},
    CertificateError, DigitallySignedStruct, DistinguishedName, Error, OtherError, SignatureScheme,
};

use crate::rtps::transport::tls::peer_lists::PeerLists;

/// Checks a dialed peer's certificate: every standard check, optionally
/// without the name match, followed by the peer lists.
#[derive(Debug)]
pub(crate) struct PeerServerVerifier {
    inner: Arc<WebPkiServerVerifier>,
    ignore_name: bool,
    lists: Option<Arc<PeerLists>>,
}

impl PeerServerVerifier {
    /// Wraps a verifier already built with the trust roots and revocation
    /// policy, so those stay configured in one place.
    pub(crate) fn new(
        inner: Arc<WebPkiServerVerifier>,
        ignore_name: bool,
        lists: Option<Arc<PeerLists>>,
    ) -> Arc<Self> {
        Arc::new(Self { inner, ignore_name, lists })
    }
}

impl ServerCertVerifier for PeerServerVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        let verified = match self.inner.verify_server_cert(
            end_entity,
            intermediates,
            server_name,
            ocsp_response,
            now,
        ) {
            // The name is checked only after the chain, validity and revocation
            // checks have passed, so a name mismatch means everything else held.
            Err(Error::InvalidCertificate(
                CertificateError::NotValidForName | CertificateError::NotValidForNameContext { .. },
            )) if self.ignore_name => ServerCertVerified::assertion(),
            result => result?,
        };
        check_lists(self.lists.as_deref(), end_entity)?;
        Ok(verified)
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

/// Checks an accepted peer's certificate: every standard check, followed by
/// the peer lists.
#[derive(Debug)]
pub(crate) struct PeerClientVerifier {
    inner: Arc<dyn ClientCertVerifier>,
    lists: Arc<PeerLists>,
}

impl PeerClientVerifier {
    pub(crate) fn new(inner: Arc<dyn ClientCertVerifier>, lists: Arc<PeerLists>) -> Arc<Self> {
        Arc::new(Self { inner, lists })
    }
}

impl ClientCertVerifier for PeerClientVerifier {
    fn offer_client_auth(&self) -> bool {
        self.inner.offer_client_auth()
    }

    fn client_auth_mandatory(&self) -> bool {
        self.inner.client_auth_mandatory()
    }

    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        self.inner.root_hint_subjects()
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        now: UnixTime,
    ) -> Result<ClientCertVerified, Error> {
        let verified = self.inner.verify_client_cert(end_entity, intermediates, now)?;
        check_lists(Some(&self.lists), end_entity)?;
        Ok(verified)
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

fn check_lists(lists: Option<&PeerLists>, cert: &CertificateDer<'_>) -> Result<(), Error> {
    match lists.map(|lists| lists.check(cert)) {
        Some(Err(rejection)) => {
            Err(Error::InvalidCertificate(CertificateError::Other(OtherError(Arc::new(rejection)))))
        }
        _ => Ok(()),
    }
}
