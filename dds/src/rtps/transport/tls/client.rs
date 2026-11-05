//! The dialing side of TLS.
//!
//! Each outbound connection takes the client configuration of the policy in
//! force when it dials, so a reload applies from the next connection on.

use std::{io, net::IpAddr, sync::Arc};

use rustls::{pki_types::ServerName, ClientConnection};

use crate::rtps::transport::{
    error::{transport_io_error, TransportErrorCode},
    tls::{config::TlsPolicy, policy::TlsPolicyHandle},
};

pub(crate) struct TlsClient {
    policy: Arc<TlsPolicyHandle>,
    server_name: Option<ServerName<'static>>,
}

impl TlsClient {
    pub(crate) fn new(policy: Arc<TlsPolicyHandle>) -> io::Result<Self> {
        let server_name =
            policy.config().server_name.clone().map(ServerName::try_from).transpose().map_err(
                |e| transport_io_error(TransportErrorCode::TlsConfigError, e.to_string()),
            )?;
        Ok(Self { policy, server_name })
    }

    /// A session to `peer` that has not started its handshake yet, with the
    /// generation of the policy it follows. Without a configured name the
    /// peer's address stands in, which sends no SNI.
    pub(crate) fn connect(&self, peer: IpAddr) -> io::Result<(ClientConnection, u64)> {
        let policy = self.policy.current();
        ClientConnection::new(Arc::clone(&policy.client), self.server_name_for(peer))
            .map(|session| (session, policy.generation))
            .map_err(|e| transport_io_error(TransportErrorCode::TlsHandshakeFailed, e.to_string()))
    }

    pub(crate) fn current_policy(&self) -> Arc<TlsPolicy> {
        self.policy.current()
    }

    /// Judges a finished handshake again when the policy was reloaded after it
    /// began, and returns the generation the connection now stands under.
    pub(crate) fn judge_again(
        &self,
        session: &ClientConnection,
        generation: u64,
        peer: IpAddr,
    ) -> Result<u64, rustls::Error> {
        let policy = self.policy.current();
        if policy.generation == generation {
            return Ok(generation);
        }
        let chain = session.peer_certificates().unwrap_or_default();
        policy.check_dialed_peer(chain, &self.server_name_for(peer))?;
        Ok(policy.generation)
    }

    pub(crate) fn server_name_for(&self, peer: IpAddr) -> ServerName<'static> {
        self.server_name.clone().unwrap_or_else(|| ServerName::from(peer))
    }
}
