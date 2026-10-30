//! A throwaway PKI for tests: CAs, leaves cut to a spec, and the files a
//! `TlsConfig` points at.

use std::{
    fs,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

use rcgen::{
    date_time_ymd, BasicConstraints, Certificate, CertificateParams,
    CertificateRevocationListParams, DnType, ExtendedKeyUsagePurpose, IsCa, KeyIdMethod, KeyPair,
    RevokedCertParams, SerialNumber,
};

use crate::rtps::transport::tls::{config::RevocationConfig, TlsConfig};

/// Unique across every certificate a test run issues, so a CRL entry names
/// exactly one certificate.
fn next_serial() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// A root or intermediate CA together with the intermediates between it and
/// its root, which every leaf it signs has to present.
pub(crate) struct TestCa {
    cert: Certificate,
    key: KeyPair,
    chain_pem: String,
    pub(crate) serial: u64,
}

impl TestCa {
    pub(crate) fn root(name: &str) -> Self {
        let key = KeyPair::generate().unwrap();
        let (params, serial) = ca_params(name);
        let cert = params.self_signed(&key).unwrap();
        Self { cert, key, chain_pem: String::new(), serial }
    }

    pub(crate) fn intermediate(&self, name: &str) -> Self {
        let key = KeyPair::generate().unwrap();
        let (params, serial) = ca_params(name);
        let cert = params.signed_by(&key, &self.cert, &self.key).unwrap();
        let chain_pem = format!("{}{}", cert.pem(), self.chain_pem);
        Self { cert, key, chain_pem, serial }
    }

    pub(crate) fn pem(&self) -> String {
        self.cert.pem()
    }

    pub(crate) fn issue(&self, spec: LeafSpec) -> TestLeaf {
        let mut params = CertificateParams::new(spec.sans).unwrap();
        params.distinguished_name.push(DnType::CommonName, spec.name);
        params.extended_key_usages = spec.usages;
        params.not_before = date_time_ymd(spec.valid_years.0, 1, 1);
        params.not_after = date_time_ymd(spec.valid_years.1, 1, 1);
        let serial = next_serial();
        params.serial_number = Some(SerialNumber::from(serial));
        let key = KeyPair::generate().unwrap();
        let cert = params.signed_by(&key, &self.cert, &self.key).unwrap();
        let chain = if spec.with_intermediates { self.chain_pem.as_str() } else { "" };
        TestLeaf {
            name: spec.name.to_string(),
            cert_pem: format!("{}{}", cert.pem(), chain),
            key_pem: key.serialize_pem(),
            der: cert.der().to_vec(),
            serial,
        }
    }

    /// A CRL from this CA revoking the certificates with the given serials,
    /// due for its next update at the start of `next_update_year`.
    pub(crate) fn crl(&self, revoked: &[u64], next_update_year: i32) -> String {
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
            next_update: date_time_ymd(next_update_year, 1, 1),
            crl_number: SerialNumber::from(next_serial()),
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

fn ca_params(name: &str) -> (CertificateParams, u64) {
    let mut params = CertificateParams::new(Vec::<String>::new()).unwrap();
    params.distinguished_name.push(DnType::CommonName, name);
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    let serial = next_serial();
    params.serial_number = Some(SerialNumber::from(serial));
    (params, serial)
}

/// What a leaf certificate looks like; the default is valid for decades and
/// usable as both client and server.
pub(crate) struct LeafSpec {
    pub(crate) name: &'static str,
    pub(crate) sans: Vec<String>,
    pub(crate) usages: Vec<ExtendedKeyUsagePurpose>,
    pub(crate) valid_years: (i32, i32),
    pub(crate) with_intermediates: bool,
}

impl LeafSpec {
    pub(crate) fn named(name: &'static str) -> Self {
        Self {
            name,
            sans: vec![format!("{name}.int2dds.test")],
            usages: vec![ExtendedKeyUsagePurpose::ServerAuth, ExtendedKeyUsagePurpose::ClientAuth],
            valid_years: (2000, 2999),
            with_intermediates: true,
        }
    }
}

pub(crate) struct TestLeaf {
    pub(crate) name: String,
    pub(crate) cert_pem: String,
    pub(crate) key_pem: String,
    pub(crate) der: Vec<u8>,
    pub(crate) serial: u64,
}

impl TestLeaf {
    /// Writes the leaf and a bundle of the `trusted` CAs into `dir`, then
    /// points a mutual-TLS config without a server name at them.
    pub(crate) fn config(&self, dir: &Path, trusted: &[&TestCa]) -> TlsConfig {
        let ca_file = dir.join(format!("{}-trust.pem", self.name));
        let cert_file = dir.join(format!("{}-cert.pem", self.name));
        let key_file = dir.join(format!("{}-key.pem", self.name));
        let bundle: String = trusted.iter().map(|ca| ca.pem()).collect();
        fs::write(&ca_file, bundle).unwrap();
        fs::write(&cert_file, &self.cert_pem).unwrap();
        fs::write(&key_file, &self.key_pem).unwrap();
        TlsConfig {
            ca_file,
            cert_file,
            key_file,
            server_name: None,
            verify_peer: true,
            revocation: None,
        }
    }
}

/// Turns on revocation checking in `config` against `crls`, written next to
/// its certificate.
pub(crate) fn with_crls(mut config: TlsConfig, crls: &[String]) -> TlsConfig {
    let crl_file = config.cert_file.with_extension("crl.pem");
    fs::write(&crl_file, crls.concat()).unwrap();
    config.revocation =
        Some(RevocationConfig { crl_file, allow_unknown_status: false, enforce_expiry: false });
    config
}
