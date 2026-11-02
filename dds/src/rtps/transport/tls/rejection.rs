//! Why a TLS handshake refused a peer, in terms an operator can act on.

use std::{fmt, io};

use rustls::{AlertDescription, CertificateError, Error};

use crate::rtps::transport::tls::peer_lists::ListRejection;

/// Why a peer was refused on certificate grounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TlsRejectionReason {
    UntrustedIssuer,
    Expired,
    NotYetValid,
    ServerNameMismatch,
    WrongPurpose,
    Revoked,
    UnknownRevocationStatus,
    ExpiredCrl,
    InvalidCrl,
    OnDenyList,
    NotOnAllowList,
    NoCertificate,
    RefusedByPeer,
    InvalidCertificate,
}

impl TlsRejectionReason {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::UntrustedIssuer => "certificate is not signed by a trusted CA",
            Self::Expired => "certificate has expired",
            Self::NotYetValid => "certificate is not valid yet",
            Self::ServerNameMismatch => "certificate does not carry the configured server name",
            Self::WrongPurpose => "certificate is not issued for this role",
            Self::Revoked => "certificate has been revoked",
            Self::UnknownRevocationStatus => {
                "revocation status is unknown (no CRL from the issuer)"
            }
            Self::ExpiredCrl => "issuer's CRL is past its next update",
            Self::InvalidCrl => "issuer's CRL is invalid (bad signature or not issued by that CA)",
            Self::OnDenyList => ListRejection::Denied.as_str(),
            Self::NotOnAllowList => ListRejection::NotAllowed.as_str(),
            Self::NoCertificate => "peer presented no certificate",
            Self::RefusedByPeer => "peer rejected this participant's certificate",
            Self::InvalidCertificate => "certificate is invalid",
        }
    }
}

impl fmt::Display for TlsRejectionReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The reason a handshake failed on certificate grounds, or `None` when the
/// failure had nothing to do with certificates. Handshake errors arrive
/// wrapped in an [`io::Error`], the way the TLS stream reports them.
pub(crate) fn rejection_reason(error: &io::Error) -> Option<TlsRejectionReason> {
    let error = error.get_ref()?.downcast_ref::<Error>()?;
    match error {
        Error::InvalidCertificate(reason) => Some(match reason {
            CertificateError::UnknownIssuer => TlsRejectionReason::UntrustedIssuer,
            CertificateError::Expired | CertificateError::ExpiredContext { .. } => {
                TlsRejectionReason::Expired
            }
            CertificateError::NotValidYet | CertificateError::NotValidYetContext { .. } => {
                TlsRejectionReason::NotYetValid
            }
            CertificateError::NotValidForName | CertificateError::NotValidForNameContext { .. } => {
                TlsRejectionReason::ServerNameMismatch
            }
            CertificateError::InvalidPurpose | CertificateError::InvalidPurposeContext { .. } => {
                TlsRejectionReason::WrongPurpose
            }
            CertificateError::Revoked => TlsRejectionReason::Revoked,
            CertificateError::UnknownRevocationStatus => {
                TlsRejectionReason::UnknownRevocationStatus
            }
            CertificateError::ExpiredRevocationList
            | CertificateError::ExpiredRevocationListContext { .. } => {
                TlsRejectionReason::ExpiredCrl
            }
            CertificateError::Other(other) => match other.0.downcast_ref::<ListRejection>() {
                Some(ListRejection::Denied) => TlsRejectionReason::OnDenyList,
                Some(ListRejection::NotAllowed) => TlsRejectionReason::NotOnAllowList,
                None => TlsRejectionReason::InvalidCertificate,
            },
            _ => TlsRejectionReason::InvalidCertificate,
        }),
        Error::InvalidCertRevocationList(_) => Some(TlsRejectionReason::InvalidCrl),
        Error::NoCertificatesPresented => Some(TlsRejectionReason::NoCertificate),
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
        ) => Some(TlsRejectionReason::RefusedByPeer),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rustls::OtherError;

    use super::*;

    fn reason_of(error: Error) -> Option<TlsRejectionReason> {
        rejection_reason(&io::Error::new(io::ErrorKind::InvalidData, error))
    }

    fn list_refusal(rejection: ListRejection) -> Error {
        Error::InvalidCertificate(CertificateError::Other(OtherError(Arc::new(rejection))))
    }

    #[test]
    fn list_refusals_keep_apart_which_list_refused() {
        assert_eq!(
            reason_of(list_refusal(ListRejection::Denied)),
            Some(TlsRejectionReason::OnDenyList)
        );
        assert_eq!(
            reason_of(list_refusal(ListRejection::NotAllowed)),
            Some(TlsRejectionReason::NotOnAllowList)
        );
    }

    #[test]
    fn a_certificate_error_without_its_own_reason_stays_a_certificate_refusal() {
        assert_eq!(
            reason_of(Error::InvalidCertificate(CertificateError::BadEncoding)),
            Some(TlsRejectionReason::InvalidCertificate)
        );
    }
}
