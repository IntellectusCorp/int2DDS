//! Derives a participant's identity bytes from the certificate it presents.
//!
//! mTLS proves only that a peer holds a certificate signed by the configured
//! CA; the GUID prefix in every RTPS message is self-written and unverified.
//! Folding the certificate into the prefix lets a receiver recompute the same
//! bytes from the authenticated connection and reject a prefix that does not
//! belong to the sender.

use ring::digest::{digest, SHA256};
use rustls::pki_types::CertificateDer;

/// Number of certificate-derived bytes carried in the GUID prefix.
pub(crate) const IDENTITY_LEN: usize = 6;

/// Leading bytes of SHA-256 over the leaf certificate's DER encoding.
pub(crate) fn identity_from_cert(cert: &CertificateDer<'_>) -> [u8; IDENTITY_LEN] {
    let hash = digest(&SHA256, cert.as_ref());
    let mut identity = [0u8; IDENTITY_LEN];
    identity.copy_from_slice(&hash.as_ref()[..IDENTITY_LEN]);
    identity
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_the_sha256_prefix_of_the_der() {
        let cert = CertificateDer::from(vec![0xAAu8; 64]);
        let expected = digest(&SHA256, &[0xAAu8; 64]);

        assert_eq!(identity_from_cert(&cert), expected.as_ref()[..IDENTITY_LEN]);
    }
}
