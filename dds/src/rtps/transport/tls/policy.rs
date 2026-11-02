//! The TLS policy a participant enforces, replaceable while it runs.

use std::{
    io,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use arc_swap::ArcSwap;
use dashmap::DashMap;
use log::error;
use rustls::pki_types::CertificateDer;

use crate::rtps::common::guid::{Guid, GuidPrefix};
use crate::rtps::transport::tls::{
    config::{OwnCertificate, TlsPolicy},
    events::{TlsDirection, TlsEvent, TlsEventLog},
    identity::IDENTITY_LEN,
    peers::TlsPeer,
    TlsConfig,
};

/// Shared by the dialing and accepting sides, so one reload reaches both.
pub(crate) struct TlsPolicyHandle {
    tls: TlsConfig,
    own: OwnCertificate,
    current: ArcSwap<TlsPolicy>,
    last_generation: Mutex<u64>,
    events: TlsEventLog,
    // The accepting thread owns its connections, so what callers may read is mirrored here.
    accepted_peers: DashMap<SocketAddr, AcceptedPeer>,
    forged_peers: Mutex<Vec<SocketAddr>>,
}

struct AcceptedPeer {
    leaf: CertificateDer<'static>,
    mismatch_reported: bool,
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
            events: TlsEventLog::default(),
            accepted_peers: DashMap::new(),
            forged_peers: Mutex::default(),
        })
    }

    pub(crate) fn config(&self) -> &TlsConfig {
        &self.tls
    }

    pub(crate) fn events(&self) -> &TlsEventLog {
        &self.events
    }

    pub(crate) fn add_accepted_peer(&self, address: SocketAddr, leaf: CertificateDer<'static>) {
        self.accepted_peers.insert(address, AcceptedPeer { leaf, mismatch_reported: false });
    }

    pub(crate) fn remove_accepted_peer(&self, address: SocketAddr) {
        self.accepted_peers.remove(&address);
    }

    pub(crate) fn clear_accepted_peers(&self) {
        self.accepted_peers.clear();
    }

    pub(crate) fn accepted_peers(&self) -> Vec<TlsPeer> {
        self.accepted_peers
            .iter()
            .map(|entry| TlsPeer::new(*entry.key(), TlsDirection::Accepted, &entry.value().leaf))
            .collect()
    }

    /// Every message of a forging peer fails alike, so its connection reports
    /// once rather than flooding the log and pushing other events out.
    pub(crate) fn report_identity_mismatch(
        &self,
        address: SocketAddr,
        identity: Option<[u8; IDENTITY_LEN]>,
        claimed_prefix: GuidPrefix,
    ) {
        if let Some(mut peer) = self.accepted_peers.get_mut(&address) {
            if std::mem::replace(&mut peer.mismatch_reported, true) {
                return;
            }
            self.forged_peers.lock().unwrap_or_else(|e| e.into_inner()).push(address);
        }
        error!(
            "TLS peer {address} claims prefix {} its certificate does not own",
            Guid::guid_prefix_to_string(&claimed_prefix)
        );
        self.events.record(TlsEvent::IdentityMismatch { address, identity, claimed_prefix });
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
