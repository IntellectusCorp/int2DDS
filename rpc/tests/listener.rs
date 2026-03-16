//! Listener-based request/reply integration tests (7.11.1.4.7~10)

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use int2dds::dcps::domain::domain_participant::DomainParticipant;
use int2dds::dcps::domain::domain_participant_factory::DomainParticipantFactory;
use int2dds::dcps::domain::qos::DomainParticipantQos;
use int2dds::dcps::infrastructure::status::StatusMask;
use int2dds::dcps::topic::type_support::DdsType;

use int2dds_rpc::entity::ServiceProxy;
use int2dds_rpc::listener::{
    ReplierListener, RequesterListener, SimpleReplierListener, SimpleRequesterListener,
};
use int2dds_rpc::params::{ReplierParams, RequesterParams};
use int2dds_rpc::replier::Replier;
use int2dds_rpc::requester::Requester;
use int2dds_rpc::sample::Sample;
use int2dds_rpc::types::{Reply, Request, SampleIdentity};

#[derive(DdsType, Debug, Clone, Default)]
#[dds_type(crate_path = "int2dds", no_additional_derives)]
struct AddRequest {
    a: i32,
    b: i32,
}

#[derive(DdsType, Debug, Clone, Default)]
#[dds_type(crate_path = "int2dds", no_additional_derives)]
struct AddResponse {
    result: i32,
}

static DOMAIN_ID: AtomicI32 = AtomicI32::new(400);

fn next_domain_id() -> i32 {
    DOMAIN_ID.fetch_add(1, Ordering::SeqCst)
}

fn create_participant(domain_id: i32) -> DomainParticipant {
    DomainParticipantFactory::get_instance()
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap()
}

/// Helper: wait for a Condvar-guarded value to become true (with timeout).
fn wait_for(pair: &(Mutex<bool>, Condvar), timeout: Duration) -> bool {
    let (lock, cvar) = pair;
    let guard = lock.lock().unwrap();
    let (guard, _) = cvar.wait_timeout_while(guard, timeout, |ready| !*ready).unwrap();
    *guard
}

struct AddHandler;

impl SimpleReplierListener<AddRequest, AddResponse> for AddHandler {
    fn process_request(
        &self,
        request: &Sample<Request<AddRequest>>,
        _related_request_id: &SampleIdentity,
    ) -> Option<AddResponse> {
        let data = request.data().unwrap();
        Some(AddResponse { result: data.data.a + data.data.b })
    }
}

/// SimpleReplierListener: the middleware auto-takes requests, calls process_request,
/// and auto-sends the returned reply.
#[test]
fn simple_replier_listener_auto_reply() {
    let domain_id = next_domain_id();
    let participant = create_participant(domain_id);

    let requester = Requester::<AddRequest, AddResponse>::new(
        RequesterParams::new(participant.clone()).service_name("SimpleReplierSvc"),
    )
    .unwrap();

    let replier = Replier::<AddRequest, AddResponse>::new(
        ReplierParams::new(participant.clone()).service_name("SimpleReplierSvc"),
    )
    .unwrap();

    // Install synchronous listener — all request handling is automatic
    replier.set_simple_replier_listener(Some(Arc::new(AddHandler))).unwrap();

    requester.wait_for_service_timeout(Duration::from_secs(3)).unwrap();

    let req_id = requester.send_request(&AddRequest { a: 10, b: 20 }).unwrap();

    // The reply should arrive automatically (processed by the listener)
    let reply = requester.receive_reply(Duration::from_secs(3)).unwrap();
    let data = reply.data().unwrap();
    assert_eq!(data.data.result, 30);
    assert_eq!(data.header.related_request_id, req_id);
}

struct AsyncRequestHandler {
    signal: Arc<(Mutex<bool>, Condvar)>,
}

impl ReplierListener<AddRequest, AddResponse> for AsyncRequestHandler {
    fn on_request_available(&self, replier: &Replier<AddRequest, AddResponse>) {
        // User manually takes and replies
        if let Ok(Some(sample)) = replier.take_request() {
            if let Ok(data) = sample.data() {
                let result = AddResponse { result: data.data.a * data.data.b };
                let _ = replier.send_reply(&result, &data.header.request_id);
            }
        }
        let (lock, cvar) = &*self.signal;
        *lock.lock().unwrap() = true;
        cvar.notify_all();
    }
}

/// ReplierListener: the middleware notifies on_request_available,
/// user must call take_request + send_reply manually.
#[test]
fn replier_listener_manual_take_and_reply() {
    let domain_id = next_domain_id();
    let participant = create_participant(domain_id);

    let requester = Requester::<AddRequest, AddResponse>::new(
        RequesterParams::new(participant.clone()).service_name("ReplierListenerSvc"),
    )
    .unwrap();

    let replier = Replier::<AddRequest, AddResponse>::new(
        ReplierParams::new(participant.clone()).service_name("ReplierListenerSvc"),
    )
    .unwrap();

    let signal = Arc::new((Mutex::new(false), Condvar::new()));
    let handler = AsyncRequestHandler { signal: signal.clone() };
    replier.set_replier_listener(Some(Arc::new(handler))).unwrap();

    requester.wait_for_service_timeout(Duration::from_secs(3)).unwrap();

    // This handler multiplies instead of adding (a * b)
    requester.send_request(&AddRequest { a: 6, b: 7 }).unwrap();

    assert!(wait_for(&signal, Duration::from_secs(3)), "listener was not called");

    let reply = requester.receive_reply(Duration::from_secs(3)).unwrap();
    let data = reply.data().unwrap();
    assert_eq!(data.data.result, 42); // 6 * 7
}

struct ReplyCollector {
    collected: Mutex<Vec<i32>>,
    signal: Arc<(Mutex<bool>, Condvar)>,
}

impl SimpleRequesterListener<AddResponse> for ReplyCollector {
    fn process_reply(
        &self,
        reply: &Sample<Reply<AddResponse>>,
        _related_request_id: &SampleIdentity,
    ) {
        if let Ok(data) = reply.data() {
            self.collected.lock().unwrap().push(data.data.result);
        }
        let (lock, cvar) = &*self.signal;
        *lock.lock().unwrap() = true;
        cvar.notify_all();
    }
}

/// SimpleRequesterListener: the middleware auto-takes each reply
/// and passes it to process_reply.
#[test]
fn simple_requester_listener_auto_take() {
    let domain_id = next_domain_id();
    let participant = create_participant(domain_id);

    let requester = Requester::<AddRequest, AddResponse>::new(
        RequesterParams::new(participant.clone()).service_name("SimpleRequesterSvc"),
    )
    .unwrap();

    let replier = Replier::<AddRequest, AddResponse>::new(
        ReplierParams::new(participant.clone()).service_name("SimpleRequesterSvc"),
    )
    .unwrap();

    let signal = Arc::new((Mutex::new(false), Condvar::new()));
    let collector =
        Arc::new(ReplyCollector { collected: Mutex::new(Vec::new()), signal: signal.clone() });
    requester.set_simple_requester_listener(Some(collector.clone())).unwrap();

    requester.wait_for_service_timeout(Duration::from_secs(3)).unwrap();

    requester.send_request(&AddRequest { a: 3, b: 4 }).unwrap();

    // Replier handles manually (only the requester side uses a listener)
    let sample = replier.receive_request(Duration::from_secs(3)).unwrap();
    let r = sample.data().unwrap();
    replier.send_reply(&AddResponse { result: r.data.a + r.data.b }, &r.header.request_id).unwrap();

    assert!(wait_for(&signal, Duration::from_secs(3)), "listener was not called");

    let results = collector.collected.lock().unwrap();
    assert_eq!(*results, vec![7]);
}

struct ReplyNotifier {
    collected: Mutex<Vec<i32>>,
    signal: Arc<(Mutex<bool>, Condvar)>,
}

impl RequesterListener<AddRequest, AddResponse> for ReplyNotifier {
    fn on_reply_available(&self, requester: &Requester<AddRequest, AddResponse>) {
        // User manually takes replies
        if let Ok(replies) = requester.take_replies(100) {
            let mut collected = self.collected.lock().unwrap();
            for reply in &replies {
                if let Ok(data) = reply.data() {
                    collected.push(data.data.result);
                }
            }
        }
        let (lock, cvar) = &*self.signal;
        *lock.lock().unwrap() = true;
        cvar.notify_all();
    }
}

/// RequesterListener: the middleware notifies on_reply_available,
/// user must call take_reply / take_replies manually.
#[test]
fn requester_listener_manual_take() {
    let domain_id = next_domain_id();
    let participant = create_participant(domain_id);

    let requester = Requester::<AddRequest, AddResponse>::new(
        RequesterParams::new(participant.clone()).service_name("RequesterListenerSvc"),
    )
    .unwrap();

    let replier = Replier::<AddRequest, AddResponse>::new(
        ReplierParams::new(participant.clone()).service_name("RequesterListenerSvc"),
    )
    .unwrap();

    let signal = Arc::new((Mutex::new(false), Condvar::new()));
    let notifier =
        Arc::new(ReplyNotifier { collected: Mutex::new(Vec::new()), signal: signal.clone() });
    requester.set_requester_listener(Some(notifier.clone())).unwrap();

    requester.wait_for_service_timeout(Duration::from_secs(3)).unwrap();

    requester.send_request(&AddRequest { a: 100, b: 200 }).unwrap();

    let sample = replier.receive_request(Duration::from_secs(3)).unwrap();
    let r = sample.data().unwrap();
    replier.send_reply(&AddResponse { result: r.data.a + r.data.b }, &r.header.request_id).unwrap();

    assert!(wait_for(&signal, Duration::from_secs(3)), "listener was not called");

    let results = notifier.collected.lock().unwrap();
    assert_eq!(*results, vec![300]);
}

/// Removing a listener by passing None should stop callbacks.
#[test]
fn remove_listener() {
    let domain_id = next_domain_id();
    let participant = create_participant(domain_id);

    let requester = Requester::<AddRequest, AddResponse>::new(
        RequesterParams::new(participant.clone()).service_name("RemoveListenerSvc"),
    )
    .unwrap();

    let replier = Replier::<AddRequest, AddResponse>::new(
        ReplierParams::new(participant.clone()).service_name("RemoveListenerSvc"),
    )
    .unwrap();

    let call_count = Arc::new(AtomicI32::new(0));
    let counter = call_count.clone();

    struct CountingHandler {
        count: Arc<AtomicI32>,
    }
    impl SimpleReplierListener<AddRequest, AddResponse> for CountingHandler {
        fn process_request(
            &self,
            request: &Sample<Request<AddRequest>>,
            _related_request_id: &SampleIdentity,
        ) -> Option<AddResponse> {
            self.count.fetch_add(1, Ordering::SeqCst);
            let data = request.data().unwrap();
            Some(AddResponse { result: data.data.a + data.data.b })
        }
    }

    replier
        .set_simple_replier_listener(Some(Arc::new(CountingHandler { count: counter })))
        .unwrap();

    requester.wait_for_service_timeout(Duration::from_secs(3)).unwrap();

    // First request — listener should fire
    requester.send_request(&AddRequest { a: 1, b: 2 }).unwrap();
    let _ = requester.receive_reply(Duration::from_secs(3)).unwrap();
    assert_eq!(call_count.load(Ordering::SeqCst), 1);

    // Remove listener
    replier.set_simple_replier_listener(None).unwrap();

    // Second request — listener should NOT fire, no auto-reply
    requester.send_request(&AddRequest { a: 3, b: 4 }).unwrap();
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(call_count.load(Ordering::SeqCst), 1, "callback should not fire after removal");
}

/// SimpleReplierListener returning None skips the reply.
struct SkipOddHandler;

impl SimpleReplierListener<AddRequest, AddResponse> for SkipOddHandler {
    fn process_request(
        &self,
        request: &Sample<Request<AddRequest>>,
        _related_request_id: &SampleIdentity,
    ) -> Option<AddResponse> {
        let data = request.data().unwrap();
        if data.data.a % 2 == 1 {
            None // skip odd requests
        } else {
            Some(AddResponse { result: data.data.a + data.data.b })
        }
    }
}

#[test]
fn simple_replier_listener_skip_reply() {
    let domain_id = next_domain_id();
    let participant = create_participant(domain_id);

    let requester = Requester::<AddRequest, AddResponse>::new(
        RequesterParams::new(participant.clone()).service_name("SkipReplySvc"),
    )
    .unwrap();

    let replier = Replier::<AddRequest, AddResponse>::new(
        ReplierParams::new(participant.clone()).service_name("SkipReplySvc"),
    )
    .unwrap();

    replier.set_simple_replier_listener(Some(Arc::new(SkipOddHandler))).unwrap();
    requester.wait_for_service_timeout(Duration::from_secs(3)).unwrap();

    // Send odd request (a=3) — should be skipped
    requester.send_request(&AddRequest { a: 3, b: 4 }).unwrap();
    // Send even request (a=4) — should get a reply
    requester.send_request(&AddRequest { a: 4, b: 5 }).unwrap();

    // Only the even request should produce a reply
    let reply = requester.receive_reply(Duration::from_secs(3)).unwrap();
    let data = reply.data().unwrap();
    assert_eq!(data.data.result, 9); // 4 + 5
}
