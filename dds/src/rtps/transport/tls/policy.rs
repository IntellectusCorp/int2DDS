//! The TLS policy a participant enforces, replaceable while it runs.

use std::{
    io,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use arc_swap::ArcSwap;
use log::error;
use rustls::pki_types::CertificateDer;

use crate::rtps::common::guid::{Guid, GuidPrefix};
use crate::rtps::transport::tls::{
    config::{OwnCertificate, TlsPolicy},
    identity::IDENTITY_LEN,
    TlsConfig,
};

/// Shared by the dialing and accepting sides, so one reload reaches both.
pub(crate) struct TlsPolicyHandle {
    tls: TlsConfig,
    own: OwnCertificate,
    current: ArcSwap<TlsPolicy>,
    last_generation: Mutex<u64>,
    forged_peers: Mutex<Vec<SocketAddr>>,
}

/// The chain a peer presented and the generation of the policy that accepted
/// it, kept so the connection can be judged again after a reload.
#[derive(Debug, Clone)]
pub(crate) struct VerifiedPeer {
    pub(crate) chain: Vec<CertificateDer<'static>>,
    pub(crate) generation: u64,
}

impl TlsPolicyHandle {
    pub(crate) fn new(tls: &TlsConfig) -> io::Result<Self> {
        let own = tls.load_own_certificate()?;
        let policy = tls.build_policy(&own, &tls.load_trust()?)?;
        Ok(Self {
            tls: tls.clone(),
            own,
            current: ArcSwap::from_pointee(policy),
            last_generation: Mutex::new(0),
            forged_peers: Mutex::default(),
        })
    }

    pub(crate) fn config(&self) -> &TlsConfig {
        &self.tls
    }

    /// Every message of a forging peer fails alike until its connection is
    /// closed, so it logs once rather than flooding the log.
    pub(crate) fn report_identity_mismatch(
        &self,
        address: SocketAddr,
        identity: Option<[u8; IDENTITY_LEN]>,
        claimed_prefix: GuidPrefix,
    ) {
        let mut forged = self.forged_peers.lock().unwrap_or_else(|e| e.into_inner());
        if forged.contains(&address) {
            return;
        }
        forged.push(address);
        drop(forged);
        error!(
            "TLS peer {address} with identity {identity:02x?} claims prefix {} its certificate \
             does not own",
            Guid::guid_prefix_to_string(&claimed_prefix)
        );
    }

    /// The connections to close: a forgery is detected deep in message
    /// processing, out of reach of the connection it came from.
    pub(crate) fn take_forged_peers(&self) -> Vec<SocketAddr> {
        std::mem::take(&mut *self.forged_peers.lock().unwrap_or_else(|e| e.into_inner()))
    }

    pub(crate) fn own_leaf(&self) -> &CertificateDer<'static> {
        self.own.leaf()
    }

    pub(crate) fn current(&self) -> Arc<TlsPolicy> {
        self.current.load_full()
    }

    /// Re-reads the CA, CRL and list files and swaps the new policy in only
    /// once every one of them loads. The certificate and key stay the ones
    /// the participant started with, since its GUID prefix is bound to them.
    pub(crate) fn reload(&self) -> io::Result<()> {
        let mut last_generation = self.last_generation.lock().unwrap_or_else(|e| e.into_inner());
        let mut policy = self.tls.build_policy(&self.own, &self.tls.load_trust()?)?;
        policy.generation = *last_generation + 1;
        self.current.store(Arc::new(policy));
        *last_generation += 1;
        Ok(())
    }
}
