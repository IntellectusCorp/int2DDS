//! TCP transport — shared facilities.
//!
//! The legacy synchronous TCP plugin previously lived here and has been
//! removed. The async TCP implementation under `tcp_async/` will be folded
//! into this module in a follow-up step; for now `tcp/` exposes only the
//! shared TLS configuration that the async plugin and the DCPS bridge
//! both consume.

pub(crate) mod tls;
