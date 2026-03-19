//! Basic request-reply integration test (7.8.1)

use std::sync::atomic::{AtomicI32, Ordering};
use std::time::Duration;

use int2dds::dcps::domain::domain_participant_factory::DomainParticipantFactory;
use int2dds::dcps::domain::qos::DomainParticipantQos;
use int2dds::dcps::infrastructure::status::StatusMask;
use int2dds::dcps::topic::type_support::DdsType;

use int2dds_rpc::entity::{RpcEntity, ServiceProxy};
use int2dds_rpc::error::DdsRpcError;
use int2dds_rpc::params::{ReplierParams, RequesterParams};
use int2dds_rpc::replier::Replier;
use int2dds_rpc::requester::Requester;
use int2dds_rpc::types::RemoteExceptionCode;

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

static DOMAIN_ID: AtomicI32 = AtomicI32::new(200);

fn next_domain_id() -> i32 {
    DOMAIN_ID.fetch_add(1, Ordering::SeqCst)
}

#[test]
fn send_request_and_receive_reply() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let service_name = "AddService";

    let requester = Requester::<AddRequest, AddResponse>::new(
        RequesterParams::new(participant.clone()).service_name(service_name),
    )
    .unwrap();

    let replier = Replier::<AddRequest, AddResponse>::new(
        ReplierParams::new(participant.clone()).service_name(service_name),
    )
    .unwrap();

    // Wait for discovery
    requester.wait_for_service_timeout(Duration::from_secs(3)).unwrap();

    // Send request
    let call = AddRequest { a: 3, b: 4 };
    let req_id = requester.send_request(&call).unwrap();

    // Replier receives request
    let sample = replier.receive_request(Duration::from_secs(3)).unwrap();
    let received = sample.data().unwrap();
    assert_eq!(received.data.a, 3);
    assert_eq!(received.data.b, 4);

    // Replier sends reply
    let related_id = received.header.request_id;
    let ret = AddResponse { result: received.data.a + received.data.b };
    replier.send_reply(&ret, &related_id).unwrap();

    // Requester receives reply
    let reply_sample = requester.receive_reply(Duration::from_secs(3)).unwrap();
    let received_reply = reply_sample.data().unwrap();
    assert_eq!(received_reply.data.result, 7);
    assert_eq!(received_reply.header.related_request_id, req_id);
    assert_eq!(received_reply.header.remote_ex, RemoteExceptionCode::Ok);
}

#[test]
fn take_reply_by_request_id() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let requester = Requester::<AddRequest, AddResponse>::new(
        RequesterParams::new(participant.clone()).service_name("TakeReplyService"),
    )
    .unwrap();

    let replier = Replier::<AddRequest, AddResponse>::new(
        ReplierParams::new(participant.clone()).service_name("TakeReplyService"),
    )
    .unwrap();

    requester.wait_for_service_timeout(Duration::from_secs(3)).unwrap();

    // Send two requests
    let call1 = AddRequest { a: 1, b: 2 };
    let id1 = requester.send_request(&call1).unwrap();

    let call2 = AddRequest { a: 10, b: 20 };
    let _id2 = requester.send_request(&call2).unwrap();

    // Replier handles both
    for _ in 0..2 {
        let sample = replier.receive_request(Duration::from_secs(3)).unwrap();
        let r = sample.data().unwrap();
        let ret = AddResponse { result: r.data.a + r.data.b };
        replier.send_reply(&ret, &r.header.request_id).unwrap();
    }

    // Requester takes reply for first request specifically
    std::thread::sleep(Duration::from_millis(100));
    let reply = requester.take_reply(&id1).unwrap().expect("reply for id1");
    let data = reply.data().unwrap();
    assert_eq!(data.data.result, 3);
}

#[test]
fn take_request_non_blocking() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let service_name = "TakeRequestService";

    let requester = Requester::<AddRequest, AddResponse>::new(
        RequesterParams::new(participant.clone()).service_name(service_name),
    )
    .unwrap();

    let replier = Replier::<AddRequest, AddResponse>::new(
        ReplierParams::new(participant.clone()).service_name(service_name),
    )
    .unwrap();

    requester.wait_for_service_timeout(Duration::from_secs(3)).unwrap();

    // No request yet — take_request returns None
    let empty = replier.take_request();
    assert!(empty.is_err() || empty.unwrap().is_none());

    // Send a request, then take it
    let call = AddRequest { a: 5, b: 6 };
    requester.send_request(&call).unwrap();
    std::thread::sleep(Duration::from_millis(100));

    let sample = replier.take_request().unwrap().expect("should have a request");
    let data = sample.data().unwrap();
    assert_eq!(data.data.a, 5);
    assert_eq!(data.data.b, 6);
}

#[test]
fn take_requests_batch() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let service_name = "TakeRequestsBatchService";

    let requester = Requester::<AddRequest, AddResponse>::new(
        RequesterParams::new(participant.clone()).service_name(service_name),
    )
    .unwrap();

    let replier = Replier::<AddRequest, AddResponse>::new(
        ReplierParams::new(participant.clone()).service_name(service_name),
    )
    .unwrap();

    requester.wait_for_service_timeout(Duration::from_secs(3)).unwrap();

    // Send 3 requests
    for i in 0..3 {
        let call = AddRequest { a: i, b: i * 10 };
        requester.send_request(&call).unwrap();
    }
    std::thread::sleep(Duration::from_millis(100));

    let samples = replier.take_requests(10).unwrap();
    assert_eq!(samples.len(), 3);
}

#[test]
fn take_replies_batch() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let service_name = "TakeRepliesBatchService";

    let requester = Requester::<AddRequest, AddResponse>::new(
        RequesterParams::new(participant.clone()).service_name(service_name),
    )
    .unwrap();

    let replier = Replier::<AddRequest, AddResponse>::new(
        ReplierParams::new(participant.clone()).service_name(service_name),
    )
    .unwrap();

    requester.wait_for_service_timeout(Duration::from_secs(3)).unwrap();

    // Send 2 requests and reply to both
    for i in 0..2 {
        let call = AddRequest { a: i, b: i + 1 };
        requester.send_request(&call).unwrap();
    }

    for _ in 0..2 {
        let sample = replier.receive_request(Duration::from_secs(3)).unwrap();
        let r = sample.data().unwrap();
        let ret = AddResponse { result: r.data.a + r.data.b };
        replier.send_reply(&ret, &r.header.request_id).unwrap();
    }

    std::thread::sleep(Duration::from_millis(100));
    let replies = requester.take_replies(10).unwrap();
    assert_eq!(replies.len(), 2);
}

#[test]
fn take_replies_for_request() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let service_name = "MultiReplierService";

    let requester = Requester::<AddRequest, AddResponse>::new(
        RequesterParams::new(participant.clone()).service_name(service_name),
    )
    .unwrap();

    let replier1 = Replier::<AddRequest, AddResponse>::new(
        ReplierParams::new(participant.clone()).service_name(service_name),
    )
    .unwrap();

    let replier2 = Replier::<AddRequest, AddResponse>::new(
        ReplierParams::new(participant.clone()).service_name(service_name),
    )
    .unwrap();

    requester.wait_for_service_timeout(Duration::from_secs(3)).unwrap();

    // Send a single request
    let call = AddRequest { a: 5, b: 3 };
    let req_id = requester.send_request(&call).unwrap();

    // Both repliers receive and reply to the same request
    for replier in [&replier1, &replier2] {
        let sample = replier.receive_request(Duration::from_secs(3)).unwrap();
        let r = sample.data().unwrap();
        let ret = AddResponse { result: r.data.a + r.data.b };
        replier.send_reply(&ret, &r.header.request_id).unwrap();
    }

    std::thread::sleep(Duration::from_millis(100));

    // Requester should receive 2 replies for the same request
    let replies = requester.take_replies_for_request(10, &req_id).unwrap();
    assert_eq!(replies.len(), 2);
    for reply in &replies {
        let data = reply.data().unwrap();
        assert_eq!(data.data.result, 8);
        assert_eq!(data.header.related_request_id, req_id);
    }
}

#[test]
fn bind_and_unbind_instance() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let mut requester = Requester::<AddRequest, AddResponse>::new(
        RequesterParams::new(participant.clone()).service_name("BindService"),
    )
    .unwrap();

    assert!(requester.get_bound_instance_name().is_none());

    requester.bind_instance("robot1".to_string()).unwrap();
    assert_eq!(requester.get_bound_instance_name(), Some("robot1"));

    requester.unbind().unwrap();
    assert!(requester.get_bound_instance_name().is_none());
}

#[test]
fn send_request_async_with_correlation() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let service_name = "FutureService";

    let requester = Requester::<AddRequest, AddResponse>::new(
        RequesterParams::new(participant.clone()).service_name(service_name),
    )
    .unwrap();

    let replier = Replier::<AddRequest, AddResponse>::new(
        ReplierParams::new(participant.clone()).service_name(service_name),
    )
    .unwrap();

    requester.wait_for_service_timeout(Duration::from_secs(3)).unwrap();

    // Send two requests via send_request_async
    let future1 = requester.send_request_async(&AddRequest { a: 10, b: 20 }).unwrap();
    let future2 = requester.send_request_async(&AddRequest { a: 100, b: 200 }).unwrap();

    // Replier handles both (order may differ from send order)
    for _ in 0..2 {
        let sample = replier.receive_request(Duration::from_secs(3)).unwrap();
        let r = sample.data().unwrap();
        let ret = AddResponse { result: r.data.a + r.data.b };
        replier.send_reply(&ret, &r.header.request_id).unwrap();
    }

    // future1.get() — blocks until the correlated reply arrives
    let reply1 = future1.get().unwrap();
    let data1 = reply1.data().unwrap();
    assert_eq!(data1.data.result, 30);

    // future2.get_timeout() — same but with explicit timeout
    let reply2 = future2.get_timeout(Duration::from_secs(3)).unwrap();
    let data2 = reply2.data().unwrap();
    assert_eq!(data2.data.result, 300);
}

#[test]
fn future_get_timeout_expires() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let requester = Requester::<AddRequest, AddResponse>::new(
        RequesterParams::new(participant.clone()).service_name("FutureTimeoutService"),
    )
    .unwrap();

    // Create a replier just so send_request succeeds, but don't reply
    let _replier = Replier::<AddRequest, AddResponse>::new(
        ReplierParams::new(participant.clone()).service_name("FutureTimeoutService"),
    )
    .unwrap();

    requester.wait_for_service_timeout(Duration::from_secs(3)).unwrap();

    let future = requester.send_request_async(&AddRequest { a: 1, b: 2 }).unwrap();

    // No reply sent — should timeout
    let result = future.get_timeout(Duration::from_millis(200));
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), DdsRpcError::Timeout | DdsRpcError::Dds(_)));
}

#[test]
fn close_requester_and_replier() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let mut requester = Requester::<AddRequest, AddResponse>::new(
        RequesterParams::new(participant.clone()).service_name("CloseService"),
    )
    .unwrap();

    assert!(!requester.is_closed());
    assert!(requester.get_request_datawriter().is_ok());
    assert!(requester.get_reply_datareader().is_ok());

    let mut replier = Replier::<AddRequest, AddResponse>::new(
        ReplierParams::new(participant.clone()).service_name("ReplierCloseService"),
    )
    .unwrap();

    assert!(!replier.is_closed());
    assert!(replier.get_request_datareader().is_ok());
    assert!(replier.get_reply_datawriter().is_ok());

    // Now close both
    requester.close().unwrap();
    assert!(requester.is_closed());
    assert!(requester.get_request_datawriter().is_err());
    assert!(requester.get_reply_datareader().is_err());

    replier.close().unwrap();
    assert!(replier.is_closed());
    assert!(replier.get_request_datareader().is_err());
    assert!(replier.get_reply_datawriter().is_err());
}
