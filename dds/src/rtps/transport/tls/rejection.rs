//! Why a TLS handshake refused a peer, in terms an operator can act on.

use std::io;

use rustls::{AlertDescription, CertificateError, Error};

use crate::rtps::transport::tls::peer_lists::ListRejection;

/// The reason a handshake failed on certificate grounds, or `None` when the
/// failure had nothing to do with certificates. Handshake errors arrive
/// wrapped in an [`io::Error`], the way the TLS stream reports them.
pub(crate) fn rejection_reason(error: &io::Error) -> Option<&'static str> {
    let error = error.get_ref()?.downcast_ref::<Error>()?;
    match error {
        Error::InvalidCertificate(reason) => Some(match reason {
            CertificateError::UnknownIssuer => "certificate is not signed by a trusted CA",
            CertificateError::Expired | CertificateError::ExpiredContext { .. } => {
                "certificate has expired"
            }
            CertificateError::NotValidYet | CertificateError::NotValidYetContext { .. } => {
                "certificate is not valid yet"
            }
            CertificateError::NotValidForName | CertificateError::NotValidForNameContext { .. } => {
                "certificate does not carry the configured server name"
            }
            CertificateError::InvalidPurpose | CertificateError::InvalidPurposeContext { .. } => {
                "certificate is not issued for this role"
            }
            CertificateError::Revoked => "certificate has been revoked",
            CertificateError::UnknownRevocationStatus => {
                "revocation status is unknown (no CRL from the issuer)"
            }
            CertificateError::ExpiredRevocationList
            | CertificateError::ExpiredRevocationListContext { .. } => {
                "issuer's CRL is past its next update"
            }
            CertificateError::Other(other) => other
                .0
                .downcast_ref::<ListRejection>()
                .map_or("certificate is invalid", |rejection| rejection.as_str()),
            _ => "certificate is invalid",
        }),
        Error::InvalidCertRevocationList(_) => {
            Some("issuer's CRL is invalid (bad signature or not issued by that CA)")
        }
        Error::NoCertificatesPresented => Some("peer presented no certificate"),
        Error::AlertReceived(
            AlertDescription::BadCertificate
            | AlertDescription::UnsupportedCertificate
            | AlertDescription::CertificateRevoked
            | AlertDescription::CertificateExpired
            | AlertDescription::CertificateUnknown
            | AlertDescription::UnknownCA
            | AlertDescription::CertificateRequired
            | AlertDescription::DecryptError
            | AlertDescription::AccessDenied,
        ) => Some("peer rejected this participant's certificate"),
        _ => None,
    }
}
