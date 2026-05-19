//! High-level TLS configuration for the TCP transport.
//!
//! `TlsConfig` describes the certificates and verification policy used
//! to wrap a TCP socket with TLS. It is built from
//! [`PropertyQosPolicy`](crate::infrastructure::qos_policy::PropertyQosPolicy)
//! values and produces the underlying `rustls::ClientConfig` /
//! `rustls::ServerConfig` on demand.
//!
//! Errors are surfaced via [`io::Error`] using the shared
//! [`TransportErrorCode`] catalogue (codes 780–789).

use std::{fs::File, io, io::BufReader, path::PathBuf, sync::Arc};

use rustls::{
    pki_types::{CertificateDer, PrivateKeyDer},
    ClientConfig, RootCertStore, ServerConfig,
};

use crate::{
    infrastructure::qos_policy::PropertyQosPolicy,
    rtps::transport::error::{transport_io_error, TransportErrorCode},
};

/// Property keys recognised by [`TlsConfig::from_property`].
pub mod keys {
    pub const CA_FILE: &str = "int2dds.tls.ca_file";
    pub const CERT_FILE: &str = "int2dds.tls.cert_file";
    pub const KEY_FILE: &str = "int2dds.tls.key_file";
    pub const SERVER_NAME: &str = "int2dds.tls.server_name";
    pub const VERIFY_PEER: &str = "int2dds.tls.verify_peer";
}

/// High-level TLS settings.
///
/// The same struct is used for both client (outgoing TCP connections) and
/// server (incoming TCP listener) sides; the active side is chosen when
/// building the corresponding `rustls` config.
#[derive(Debug, Clone)]
pub struct TlsConfig {
    /// Path to a PEM file containing one or more trusted CA certificates.
    pub ca_file: PathBuf,
    /// Path to a PEM file containing this peer's certificate chain.
    pub cert_file: PathBuf,
    /// Path to a PEM file containing this peer's private key.
    pub key_file: PathBuf,
    /// SNI server name to present when connecting (client side).
    /// On the server side this field is informational only.
    pub server_name: String,
    /// Require the remote peer to present a certificate (mutual TLS).
    /// Server side: enforce client certificates.
    /// Client side: always verifies the server certificate (rustls default).
    pub verify_peer: bool,
}

impl TlsConfig {
    /// Try to read a TLS config from a [`PropertyQosPolicy`].
    ///
    /// * `Ok(None)` — no TLS-related property is present, caller should
    ///   fall back to plain TCP.
    /// * `Ok(Some(_))` — a complete config was assembled.
    /// * `Err(_)` — at least one TLS property exists but a required one is
    ///   missing.
    pub fn from_property(property: &PropertyQosPolicy) -> io::Result<Option<Self>> {
        let any_tls_key = [keys::CA_FILE, keys::CERT_FILE, keys::KEY_FILE]
            .iter()
            .any(|k| property.find_property(k).is_some());
        if !any_tls_key {
            return Ok(None);
        }

        let ca_file = require_property(property, keys::CA_FILE)?.into();
        let cert_file = require_property(property, keys::CERT_FILE)?.into();
        let key_file = require_property(property, keys::KEY_FILE)?.into();
        let server_name =
            property.find_property(keys::SERVER_NAME).unwrap_or("localhost").to_string();
        let verify_peer =
            property.find_property(keys::VERIFY_PEER).map(|v| v == "true").unwrap_or(false);

        Ok(Some(Self { ca_file, cert_file, key_file, server_name, verify_peer }))
    }

    fn load_root_store(&self) -> io::Result<RootCertStore> {
        let mut reader = BufReader::new(open_pem(&self.ca_file)?);
        let mut store = RootCertStore::empty();
        for cert in rustls_pemfile::certs(&mut reader) {
            let cert = cert.map_err(|e| pem_error(&self.ca_file, e))?;
            store.add(cert).map_err(rustls_to_io)?;
        }
        Ok(store)
    }

    fn load_cert_chain(&self) -> io::Result<Vec<CertificateDer<'static>>> {
        let mut reader = BufReader::new(open_pem(&self.cert_file)?);
        let mut chain = Vec::new();
        for cert in rustls_pemfile::certs(&mut reader) {
            chain.push(cert.map_err(|e| pem_error(&self.cert_file, e))?);
        }
        if chain.is_empty() {
            return Err(transport_io_error(
                TransportErrorCode::TlsInvalidPem,
                format!("no certificates found in {:?}", self.cert_file),
            ));
        }
        Ok(chain)
    }

    fn load_private_key(&self) -> io::Result<PrivateKeyDer<'static>> {
        let mut reader = BufReader::new(open_pem(&self.key_file)?);
        match rustls_pemfile::private_key(&mut reader).map_err(|e| pem_error(&self.key_file, e))? {
            Some(key) => Ok(key),
            None => Err(transport_io_error(
                TransportErrorCode::TlsInvalidPem,
                format!("no private key found in {:?}", self.key_file),
            )),
        }
    }

    /// Build a `rustls::ClientConfig` for outgoing TCP connections.
    pub fn build_client_config(&self) -> io::Result<Arc<ClientConfig>> {
        let roots = self.load_root_store()?;
        let cert_chain = self.load_cert_chain()?;
        let key = self.load_private_key()?;

        let cfg = ClientConfig::builder()
            .with_root_certificates(roots)
            .with_client_auth_cert(cert_chain, key)
            .map_err(rustls_to_io)?;
        Ok(Arc::new(cfg))
    }

    /// Build a `rustls::ServerConfig` for the incoming TCP listener.
    pub fn build_server_config(&self) -> io::Result<Arc<ServerConfig>> {
        let roots = self.load_root_store()?;
        let cert_chain = self.load_cert_chain()?;
        let key = self.load_private_key()?;

        let builder = ServerConfig::builder();
        let cfg = if self.verify_peer {
            let verifier =
                rustls::server::WebPkiClientVerifier::builder(Arc::new(roots)).build().map_err(
                    |e| transport_io_error(TransportErrorCode::TlsConfigError, e.to_string()),
                )?;
            builder
                .with_client_cert_verifier(verifier)
                .with_single_cert(cert_chain, key)
                .map_err(rustls_to_io)?
        } else {
            builder.with_no_client_auth().with_single_cert(cert_chain, key).map_err(rustls_to_io)?
        };
        Ok(Arc::new(cfg))
    }
}

// ── helpers ────────────────────────────────────────────────────────────────

fn require_property<'a>(p: &'a PropertyQosPolicy, key: &'static str) -> io::Result<&'a str> {
    p.find_property(key).ok_or_else(|| {
        transport_io_error(
            TransportErrorCode::TlsMissingProperty,
            format!("TLS property '{}' is required", key),
        )
    })
}

fn open_pem(path: &PathBuf) -> io::Result<File> {
    File::open(path).map_err(|e| {
        transport_io_error(
            TransportErrorCode::TlsFileIoError,
            format!("failed to open {:?}: {}", path, e),
        )
    })
}

fn pem_error(path: &PathBuf, e: impl std::fmt::Display) -> io::Error {
    transport_io_error(
        TransportErrorCode::TlsInvalidPem,
        format!("invalid PEM in {:?}: {}", path, e),
    )
}

fn rustls_to_io(e: rustls::Error) -> io::Error {
    transport_io_error(TransportErrorCode::TlsConfigError, e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pq(pairs: &[(&str, &str)]) -> PropertyQosPolicy {
        let mut p = PropertyQosPolicy::default();
        for (k, v) in pairs {
            p.add_property(*k, *v, false);
        }
        p
    }

    #[test]
    fn no_tls_keys_returns_none() {
        let p = pq(&[("int2dds.transport", "tcp")]);
        assert!(TlsConfig::from_property(&p).unwrap().is_none());
    }

    #[test]
    fn partial_tls_keys_is_error() {
        // ca_file present, others missing.
        let p = pq(&[(keys::CA_FILE, "/etc/ssl/ca.pem")]);
        let err = TlsConfig::from_property(&p).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        let msg = format!("{err}");
        assert!(msg.contains(keys::CERT_FILE), "expected mention of cert_file in {msg}");
    }

    #[test]
    fn full_tls_keys_parsed() {
        let p = pq(&[
            (keys::CA_FILE, "/etc/ssl/ca.pem"),
            (keys::CERT_FILE, "/etc/ssl/cert.pem"),
            (keys::KEY_FILE, "/etc/ssl/key.pem"),
            (keys::SERVER_NAME, "gateway.example.com"),
            (keys::VERIFY_PEER, "true"),
        ]);
        let cfg = TlsConfig::from_property(&p).unwrap().unwrap();
        assert_eq!(cfg.ca_file, PathBuf::from("/etc/ssl/ca.pem"));
        assert_eq!(cfg.cert_file, PathBuf::from("/etc/ssl/cert.pem"));
        assert_eq!(cfg.key_file, PathBuf::from("/etc/ssl/key.pem"));
        assert_eq!(cfg.server_name, "gateway.example.com");
        assert!(cfg.verify_peer);
    }

    #[test]
    fn server_name_defaults_to_localhost() {
        let p = pq(&[(keys::CA_FILE, "/a"), (keys::CERT_FILE, "/b"), (keys::KEY_FILE, "/c")]);
        let cfg = TlsConfig::from_property(&p).unwrap().unwrap();
        assert_eq!(cfg.server_name, "localhost");
        assert!(!cfg.verify_peer);
    }
}
