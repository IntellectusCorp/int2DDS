//! RPC listener traits for callback-based request/reply processing (7.11.1.4.6~10)

use crate::sample::Sample;
use crate::types::{Reply, Request, SampleIdentity};

/// Synchronous request handler for a Replier. (7.11.1.4.7)
/// The middleware calls `process_request` when a request arrives and
/// automatically sends the returned value as the reply.
/// Return `None` to skip sending a reply for this request.
pub trait SimpleReplierListener<TReq, TRep>: Send + Sync + 'static {
    fn process_request(
        &self,
        request: &Sample<Request<TReq>>,
        related_request_id: &SampleIdentity,
    ) -> Option<TRep>;
}

/// Asynchronous request notification for a Replier. (7.11.1.4.8)
/// The middleware calls `on_request_available` when requests arrive;
/// the user must call `take_request` / `send_reply` manually.
pub trait ReplierListener<TReq, TRep>: Send + Sync + 'static {
    fn on_request_available(&self, replier: &crate::replier::Replier<TReq, TRep>);
}

/// Reply notification with automatic take for a Requester. (7.11.1.4.9)
/// The middleware takes the reply and passes it directly to `process_reply`.
pub trait SimpleRequesterListener<TRep>: Send + Sync + 'static {
    fn process_reply(&self, reply: &Sample<Reply<TRep>>, related_request_id: &SampleIdentity);
}

/// Reply arrival notification for a Requester. (7.11.1.4.10)
/// The middleware calls `on_reply_available` when replies arrive;
/// the user must call `take_reply` / `take_replies` manually.
pub trait RequesterListener<TReq, TRep>: Send + Sync + 'static {
    fn on_reply_available(&self, requester: &crate::requester::Requester<TReq, TRep>);
}
