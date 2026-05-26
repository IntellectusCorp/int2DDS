//! Send-path abstraction shared by the sync and async TCP senders.
//!
//! The transport plugin holds both a synchronous and an asynchronous
//! outbound implementation and dispatches between them based on each
//! DataWriter's `PublishModeQosPolicy`. The trait gives them a common
//! surface so the plugin's call sites do not branch on the concrete
//! sender type.
//!
//! ## Design notes
//!
//! - **Static dispatch.** Methods take `self: &Arc<Self>` so each impl can
//!   clone its `Arc` into spawned tasks / connect threads without going
//!   through `dyn`. The plugin will keep concrete `Arc<AsyncTcpSender>` and
//!   `Arc<SyncTcpSender>` handles side-by-side and `match` on the writer's
//!   publish mode; no `Box<dyn TcpSenderImpl>` is required.
//! - **No I/O coalescing in this trait.** Coalescing is the responsibility
//!   of the async path's built-in default send scheduler (the per-connection
//!   `writer_task`). The sync path bypasses that scheduler entirely — the
//!   user thread performs the `writev` inline — which mirrors how
//!   synchronous writes opt out of any flow-controller behavior.
//! - **Lifecycle is plugin-managed.** `new()` / `shutdown()` are not in the
//!   trait; the plugin owns construction order and the tokio runtime, and
//!   each impl exposes its own concrete shutdown shape.

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;

/// Common send-side contract for the TCP transport.
///
/// Implementors enqueue or directly write RTPS frames to remote peers. The
/// exact dispatch semantics (queued + coalesced vs. inline blocking) are
/// impl-defined and selected per-DataWriter by the surrounding plugin.
pub(crate) trait TcpSenderImpl {
    /// Send an RTPS frame to a peer's discovery port. Sync, fire-and-forget;
    /// on cache miss the impl is expected to establish a fresh connection
    /// and queue/replay the frame.
    fn send_to_discovery(self: &Arc<Self>, addr: &SocketAddr, data: &[u8]) -> io::Result<()>;

    /// Send an RTPS frame to a peer's user-data port. Same fire-and-forget
    /// semantics as [`send_to_discovery`](Self::send_to_discovery).
    fn send_to_user_data(self: &Arc<Self>, addr: &SocketAddr, data: &[u8]) -> io::Result<()>;

    /// Evict all cached state for `addr` and notify the dead-peer channel.
    /// Used on keepalive failure or explicit peer eviction.
    fn disconnect_peer(&self, addr: SocketAddr);
}
