//! The dialing side of TLS, built once per participant.
//!
//! Every outbound connection shares one client configuration, so the
//! certificates, CRLs and lists it was built from are read when the
//! participant starts and apply to dialing and accepting alike.

use std::{io, net::IpAddr, sync::Arc};

use rustls::{pki_types::ServerName, ClientConfig, ClientConnection};

use crate::rtps::transport::{
    error::{transport_io_error, TransportErrorCode},
    tls::TlsConfig,
};

pub(crate) struct TlsClient {
    config: Arc<ClientConfig>,
    server_name: Option<ServerName<'static>>,
}

impl TlsClient {
    pub(crate) fn new(tls: &TlsConfig) -> io::Result<Self> {
        let config = tls.build_client_config()?;
        let server_name =
            tls.server_name.clone().map(ServerName::try_from).transpose().map_err(|e| {
                transport_io_error(TransportErrorCode::TlsConfigError, e.to_string())
            })?;
        Ok(Self { config, server_name })
    }

    /// A session to `peer` that has not started its handshake yet. Without a
    /// configured name the peer's address stands in, which sends no SNI.
    pub(crate) fn connect(&self, peer: IpAddr) -> io::Result<ClientConnection> {
        let server_name = self.server_name.clone().unwrap_or_else(|| ServerName::from(peer));
        ClientConnection::new(Arc::clone(&self.config), server_name)
            .map_err(|e| transport_io_error(TransportErrorCode::TlsHandshakeFailed, e.to_string()))
    }
}
