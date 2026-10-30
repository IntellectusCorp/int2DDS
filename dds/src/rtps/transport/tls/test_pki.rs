//! A throwaway PKI for tests: CAs, leaves cut to a spec, and the files a
//! `TlsConfig` points at.

use std::{fs, path::Path};

use rcgen::{
    date_time_ymd, BasicConstraints, Certificate, CertificateParams, DnType,
    ExtendedKeyUsagePurpose, IsCa, KeyPair,
};

use crate::rtps::transport::tls::TlsConfig;

/// A root or intermediate CA together with the intermediates between it and
/// its root, which every leaf it signs has to present.
pub(crate) struct TestCa {
    cert: Certificate,
    key: KeyPair,
    chain_pem: String,
}

impl TestCa {
    pub(crate) fn root(name: &str) -> Self {
        let key = KeyPair::generate().unwrap();
        let cert = ca_params(name).self_signed(&key).unwrap();
        Self { cert, key, chain_pem: String::new() }
    }

    pub(crate) fn intermediate(&self, name: &str) -> Self {
        let key = KeyPair::generate().unwrap();
        let cert = ca_params(name).signed_by(&key, &self.cert, &self.key).unwrap();
        let chain_pem = format!("{}{}", cert.pem(), self.chain_pem);
        Self { cert, key, chain_pem }
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
        let key = KeyPair::generate().unwrap();
        let cert = params.signed_by(&key, &self.cert, &self.key).unwrap();
        let chain = if spec.with_intermediates { self.chain_pem.as_str() } else { "" };
        TestLeaf {
            name: spec.name.to_string(),
            cert_pem: format!("{}{}", cert.pem(), chain),
            key_pem: key.serialize_pem(),
            der: cert.der().to_vec(),
        }
    }
}

fn ca_params(name: &str) -> CertificateParams {
    let mut params = CertificateParams::new(Vec::<String>::new()).unwrap();
    params.distinguished_name.push(DnType::CommonName, name);
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params
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
        TlsConfig { ca_file, cert_file, key_file, server_name: None, verify_peer: true }
    }
}
