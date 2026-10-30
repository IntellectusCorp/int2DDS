//! Server certificate verification for peers that are dialed by address.
//!
//! A participant reaches its peers through discovered locators, not names, so
//! there is no name a peer's certificate could be expected to carry. Identity
//! is established by binding the GUID prefix to the certificate instead.

use std::{io, sync::Arc};

use rustls::{
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    client::WebPkiServerVerifier,
    pki_types::{CertificateDer, ServerName, UnixTime},
    CertificateError, DigitallySignedStruct, Error, RootCertStore, SignatureScheme,
};

use crate::rtps::transport::error::{transport_io_error, TransportErrorCode};

/// Runs every standard server certificate check except the name match.
#[derive(Debug)]
pub(crate) struct NameAgnosticServerVerifier {
    inner: Arc<WebPkiServerVerifier>,
}

impl NameAgnosticServerVerifier {
    pub(crate) fn new(roots: RootCertStore) -> io::Result<Arc<Self>> {
        let inner = WebPkiServerVerifier::builder(Arc::new(roots))
            .build()
            .map_err(|e| transport_io_error(TransportErrorCode::TlsConfigError, e.to_string()))?;
        Ok(Arc::new(Self { inner }))
    }
}

impl ServerCertVerifier for NameAgnosticServerVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        match self.inner.verify_server_cert(
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
            )) => Ok(ServerCertVerified::assertion()),
            result => result,
        }
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
