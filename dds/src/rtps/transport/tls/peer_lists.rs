//! Allow and deny lists that narrow which validly signed peers may connect.
//!
//! Entries are SHA-256 fingerprints of a peer's leaf certificate, one per line,
//! in the form `openssl x509 -fingerprint -sha256` prints them.

use std::{
    collections::HashSet,
    fmt, fs, io,
    path::{Path, PathBuf},
};

use ring::digest::{digest, SHA256};
use rustls::pki_types::CertificateDer;

use crate::rtps::transport::{
    error::{transport_io_error, TransportErrorCode},
    tls::TlsConfig,
};

type Fingerprint = [u8; 32];

#[derive(Debug)]
pub(crate) struct PeerLists {
    allow: Option<HashSet<Fingerprint>>,
    deny: HashSet<Fingerprint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ListRejection {
    Denied,
    NotAllowed,
}

impl ListRejection {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Denied => "certificate is on the deny list",
            Self::NotAllowed => "certificate is not on the allow list",
        }
    }
}

impl fmt::Display for ListRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::error::Error for ListRejection {}

impl PeerLists {
    /// The lists `tls` points at, or `None` when it configures neither.
    pub(crate) fn load(tls: &TlsConfig) -> io::Result<Option<Self>> {
        if tls.allow_file.is_none() && tls.deny_file.is_none() {
            return Ok(None);
        }
        let allow = tls.allow_file.as_deref().map(read_fingerprints).transpose()?;
        let deny = tls.deny_file.as_deref().map(read_fingerprints).transpose()?.unwrap_or_default();
        Ok(Some(Self { allow, deny }))
    }

    /// Deny wins over allow; with an allow list, anything not on it is refused.
    pub(crate) fn check(&self, cert: &CertificateDer<'_>) -> Result<(), ListRejection> {
        let mut fingerprint = [0u8; 32];
        fingerprint.copy_from_slice(digest(&SHA256, cert.as_ref()).as_ref());
        if self.deny.contains(&fingerprint) {
            return Err(ListRejection::Denied);
        }
        match &self.allow {
            Some(allow) if !allow.contains(&fingerprint) => Err(ListRejection::NotAllowed),
            _ => Ok(()),
        }
    }
}

/// An empty file is refused, since an empty allow list would refuse everyone.
fn read_fingerprints(path: &Path) -> io::Result<HashSet<Fingerprint>> {
    let text = fs::read_to_string(path).map_err(|e| {
        transport_io_error(
            TransportErrorCode::TlsFileIoError,
            format!("failed to read {path:?}: {e}"),
        )
    })?;
    let mut fingerprints = HashSet::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let value = line.rsplit('=').next().unwrap_or(line).replace(':', "");
        let fingerprint = parse_hex(value.trim()).ok_or_else(|| {
            transport_io_error(
                TransportErrorCode::TlsConfigError,
                format!("line {} of {path:?} is not a SHA-256 fingerprint", index + 1),
            )
        })?;
        fingerprints.insert(fingerprint);
    }
    if fingerprints.is_empty() {
        return Err(transport_io_error(
            TransportErrorCode::TlsConfigError,
            format!("no fingerprints found in {:?}", PathBuf::from(path)),
        ));
    }
    Ok(fingerprints)
}

fn parse_hex(value: &str) -> Option<Fingerprint> {
    if value.len() != 64 || !value.is_ascii() {
        return None;
    }
    let mut fingerprint = [0u8; 32];
    for (byte, pair) in fingerprint.iter_mut().zip(value.as_bytes().chunks(2)) {
        *byte = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(fingerprint)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fingerprint_line(cert: &CertificateDer<'_>, colons: bool) -> String {
        let hex: Vec<String> =
            digest(&SHA256, cert.as_ref()).as_ref().iter().map(|b| format!("{b:02X}")).collect();
        if colons {
            hex.join(":")
        } else {
            hex.concat().to_lowercase()
        }
    }

    fn config(dir: &Path, allow: Option<&str>, deny: Option<&str>) -> TlsConfig {
        let write = |name: &str, text: Option<&str>| {
            text.map(|text| {
                let path = dir.join(name);
                fs::write(&path, text).unwrap();
                path
            })
        };
        TlsConfig {
            ca_file: PathBuf::new(),
            cert_file: PathBuf::new(),
            key_file: PathBuf::new(),
            server_name: None,
            verify_peer: true,
            revocation: None,
            allow_file: write("allow.txt", allow),
            deny_file: write("deny.txt", deny),
        }
    }

    #[test]
    fn lists_accept_openssl_output_and_put_deny_first() {
        let dir = tempfile::tempdir().unwrap();
        let allowed = CertificateDer::from(vec![1u8; 32]);
        let both = CertificateDer::from(vec![2u8; 32]);
        let unlisted = CertificateDer::from(vec![3u8; 32]);
        let allow = format!(
            "# nodes\n\nsha256 Fingerprint={}\n{}\n",
            fingerprint_line(&allowed, true),
            fingerprint_line(&both, false)
        );
        let deny = format!("  {}  \n", fingerprint_line(&both, true).to_lowercase());

        let lists =
            PeerLists::load(&config(dir.path(), Some(&allow), Some(&deny))).unwrap().unwrap();
        assert_eq!(lists.check(&allowed), Ok(()));
        assert_eq!(lists.check(&both), Err(ListRejection::Denied));
        assert_eq!(lists.check(&unlisted), Err(ListRejection::NotAllowed));

        let deny_only = PeerLists::load(&config(dir.path(), None, Some(&deny))).unwrap().unwrap();
        assert_eq!(deny_only.check(&unlisted), Ok(()));
        assert_eq!(deny_only.check(&both), Err(ListRejection::Denied));

        assert!(PeerLists::load(&config(dir.path(), None, None)).unwrap().is_none());
    }

    #[test]
    fn a_malformed_or_empty_list_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let valid = fingerprint_line(&CertificateDer::from(vec![1u8; 32]), true);
        let cases = [
            (format!("{valid}\nAB:CD\n"), "line 2"),
            (format!("G{}\n", &valid[1..]), "line 1"),
            ("# only a comment\n\n".to_string(), "no fingerprints"),
        ];

        for (text, expected) in cases {
            let error = PeerLists::load(&config(dir.path(), Some(&text), None)).err().unwrap();
            assert!(error.to_string().contains(expected), "{error}");
        }
    }
}
