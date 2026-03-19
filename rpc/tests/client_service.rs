//! Client/Service/Server integration tests (7.9.1, 7.11.1.5)

use std::sync::atomic::{AtomicI32, Ordering};
use std::thread;
use std::time::Duration;

use int2dds::dcps::domain::domain_participant_factory::DomainParticipantFactory;
use int2dds::dcps::domain::qos::DomainParticipantQos;
use int2dds::dcps::infrastructure::status::StatusMask;
use int2dds::dcps::topic::type_support::DdsType;

use int2dds_rpc::client::{Client, ClientParams};
use int2dds_rpc::entity::{RpcEntity, ServiceProxy};
use int2dds_rpc::server::{Server, ServerParams};
use int2dds_rpc::service::{
    RequestHandler, Service, ServiceEndpoint, ServiceParams, ServiceStatus,
};
use int2dds_rpc::types::RemoteExceptionCode;

#[derive(DdsType, Debug, Clone, Default)]
#[dds_type(crate_path = "int2dds", no_additional_derives)]
struct AddCall {
    a: i32,
    b: i32,
}

#[derive(DdsType, Debug, Clone, Default)]
#[dds_type(crate_path = "int2dds", no_additional_derives)]
struct AddReturn {
    result: i32,
}

#[derive(DdsType, Debug, Clone, Default)]
#[dds_type(crate_path = "int2dds", no_additional_derives)]
struct EchoCall {
    msg: String,
}

#[derive(DdsType, Debug, Clone, Default)]
#[dds_type(crate_path = "int2dds", no_additional_derives)]
struct EchoReturn {
    msg: String,
}

struct AddHandler;

impl RequestHandler<AddCall, AddReturn> for AddHandler {
    fn handle_request(&self, request: &AddCall) -> (AddReturn, RemoteExceptionCode) {
        (AddReturn { result: request.a + request.b }, RemoteExceptionCode::Ok)
    }
}

struct EchoHandler;

impl RequestHandler<EchoCall, EchoReturn> for EchoHandler {
    fn handle_request(&self, request: &EchoCall) -> (EchoReturn, RemoteExceptionCode) {
        (EchoReturn { msg: request.msg.clone() }, RemoteExceptionCode::Ok)
    }
}

struct UnsupportedHandler;

impl RequestHandler<AddCall, AddReturn> for UnsupportedHandler {
    fn handle_request(&self, _request: &AddCall) -> (AddReturn, RemoteExceptionCode) {
        (AddReturn { result: 0 }, RemoteExceptionCode::Unsupported)
    }
}

static DOMAIN_ID: AtomicI32 = AtomicI32::new(300);

fn next_domain_id() -> i32 {
    DOMAIN_ID.fetch_add(1, Ordering::SeqCst)
}

fn make_participant(
    domain_id: i32,
) -> int2dds::dcps::domain::domain_participant::DomainParticipant {
    DomainParticipantFactory::get_instance()
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap()
}

#[test]
fn client_service_basic_roundtrip() {
    let domain_id = next_domain_id();
    let participant = make_participant(domain_id);
    let svc_name = "BasicRoundtrip";

    let service =
        Service::new(ServiceParams::new(participant.clone()).service_name(svc_name), AddHandler)
            .unwrap();

    let client = Client::<AddCall, AddReturn>::new(
        ClientParams::new(participant.clone()).service_name(svc_name),
    )
    .unwrap();

    client.wait_for_service_timeout(Duration::from_secs(5)).unwrap();

    let mut server = Server::new(ServerParams::new());
    server.add_service(service);

    // run server in background
    let handle = thread::spawn(move || server.run_for(Duration::from_secs(2)));

    let _id = client.send_request(&AddCall { a: 10, b: 20 }).unwrap();
    let reply = client.receive_reply(Duration::from_secs(5)).unwrap();
    let data = reply.data().unwrap();
    assert_eq!(data.data.result, 30);
    assert_eq!(data.header.remote_ex, RemoteExceptionCode::Ok);

    handle.join().unwrap().unwrap();
}

#[test]
fn client_service_sequential_requests() {
    let domain_id = next_domain_id();
    let participant = make_participant(domain_id);
    let svc_name = "SequentialReqs";

    let service =
        Service::new(ServiceParams::new(participant.clone()).service_name(svc_name), AddHandler)
            .unwrap();

    let client = Client::<AddCall, AddReturn>::new(
        ClientParams::new(participant.clone()).service_name(svc_name),
    )
    .unwrap();

    client.wait_for_service_timeout(Duration::from_secs(5)).unwrap();

    let mut server = Server::new(ServerParams::new());
    server.add_service(service);
    let handle = thread::spawn(move || server.run_for(Duration::from_secs(5)));

    for i in 0..3 {
        client.send_request(&AddCall { a: i, b: i * 10 }).unwrap();
        let reply = client.receive_reply(Duration::from_secs(5)).unwrap();
        let data = reply.data().unwrap();
        assert_eq!(data.data.result, i + i * 10);
    }

    handle.join().unwrap().unwrap();
}

#[test]
fn client_service_void_operation() {
    let domain_id = next_domain_id();
    let participant = make_participant(domain_id);
    let svc_name = "VoidOp";

    // Handler returns default (zero) response, simulating void
    let service =
        Service::new(ServiceParams::new(participant.clone()).service_name(svc_name), AddHandler)
            .unwrap();

    let client = Client::<AddCall, AddReturn>::new(
        ClientParams::new(participant.clone()).service_name(svc_name),
    )
    .unwrap();

    client.wait_for_service_timeout(Duration::from_secs(5)).unwrap();

    let mut server = Server::new(ServerParams::new());
    server.add_service(service);
    let handle = thread::spawn(move || server.run_for(Duration::from_secs(2)));

    // a=0, b=0 → result=0, just verifying roundtrip completes
    client.send_request(&AddCall { a: 0, b: 0 }).unwrap();
    let reply = client.receive_reply(Duration::from_secs(5)).unwrap();
    let data = reply.data().unwrap();
    assert_eq!(data.data.result, 0);

    handle.join().unwrap().unwrap();
}

#[test]
fn client_service_async_future() {
    let domain_id = next_domain_id();
    let participant = make_participant(domain_id);
    let svc_name = "AsyncFuture";

    let service =
        Service::new(ServiceParams::new(participant.clone()).service_name(svc_name), AddHandler)
            .unwrap();

    let client = Client::<AddCall, AddReturn>::new(
        ClientParams::new(participant.clone()).service_name(svc_name),
    )
    .unwrap();

    client.wait_for_service_timeout(Duration::from_secs(5)).unwrap();

    let mut server = Server::new(ServerParams::new());
    server.add_service(service);
    let handle = thread::spawn(move || server.run_for(Duration::from_secs(2)));

    let future = client.send_request_async(&AddCall { a: 7, b: 8 }).unwrap();
    let reply = future.get().unwrap();
    let data = reply.data().unwrap();
    assert_eq!(data.data.result, 15);

    handle.join().unwrap().unwrap();
}

#[test]
fn client_service_async_future_timeout() {
    let domain_id = next_domain_id();
    let participant = make_participant(domain_id);
    let svc_name = "AsyncFutureTimeout";

    let service =
        Service::new(ServiceParams::new(participant.clone()).service_name(svc_name), AddHandler)
            .unwrap();

    let client = Client::<AddCall, AddReturn>::new(
        ClientParams::new(participant.clone()).service_name(svc_name),
    )
    .unwrap();

    client.wait_for_service_timeout(Duration::from_secs(5)).unwrap();

    let mut server = Server::new(ServerParams::new());
    server.add_service(service);
    let handle = thread::spawn(move || server.run_for(Duration::from_secs(2)));

    let future = client.send_request_async(&AddCall { a: 3, b: 4 }).unwrap();
    let reply = future.get_timeout(Duration::from_secs(5)).unwrap();
    let data = reply.data().unwrap();
    assert_eq!(data.data.result, 7);

    handle.join().unwrap().unwrap();
}

#[test]
fn client_service_async_no_reply_timeout() {
    let domain_id = next_domain_id();
    let participant = make_participant(domain_id);
    let svc_name = "AsyncNoReply";

    // Create service but don't run server → no dispatch → no reply
    let _service =
        Service::new(ServiceParams::new(participant.clone()).service_name(svc_name), AddHandler)
            .unwrap();

    let client = Client::<AddCall, AddReturn>::new(
        ClientParams::new(participant.clone()).service_name(svc_name),
    )
    .unwrap();

    client.wait_for_service_timeout(Duration::from_secs(5)).unwrap();

    let future = client.send_request_async(&AddCall { a: 1, b: 2 }).unwrap();
    let result = future.get_timeout(Duration::from_millis(300));
    assert!(result.is_err());
}

#[test]
fn client_service_exception_code() {
    let domain_id = next_domain_id();
    let participant = make_participant(domain_id);
    let svc_name = "ExceptionCode";

    let service = Service::new(
        ServiceParams::new(participant.clone()).service_name(svc_name),
        UnsupportedHandler,
    )
    .unwrap();

    let client = Client::<AddCall, AddReturn>::new(
        ClientParams::new(participant.clone()).service_name(svc_name),
    )
    .unwrap();

    client.wait_for_service_timeout(Duration::from_secs(5)).unwrap();

    let mut server = Server::new(ServerParams::new());
    server.add_service(service);
    let handle = thread::spawn(move || server.run_for(Duration::from_secs(2)));

    client.send_request(&AddCall { a: 1, b: 2 }).unwrap();
    let reply = client.receive_reply(Duration::from_secs(5)).unwrap();
    let data = reply.data().unwrap();
    assert_eq!(data.header.remote_ex, RemoteExceptionCode::Unsupported);

    handle.join().unwrap().unwrap();
}

#[test]
fn client_receive_timeout_no_service() {
    let domain_id = next_domain_id();
    let participant = make_participant(domain_id);

    let client = Client::<AddCall, AddReturn>::new(
        ClientParams::new(participant.clone()).service_name("NoService"),
    )
    .unwrap();

    // No service exists → receive_reply should timeout
    let result = client.receive_reply(Duration::from_millis(200));
    assert!(result.is_err());
}

#[test]
fn server_multiple_services() {
    let domain_id = next_domain_id();
    let participant = make_participant(domain_id);

    let add_service =
        Service::new(ServiceParams::new(participant.clone()).service_name("MultiAdd"), AddHandler)
            .unwrap();

    let echo_service = Service::new(
        ServiceParams::new(participant.clone()).service_name("MultiEcho"),
        EchoHandler,
    )
    .unwrap();

    let add_client = Client::<AddCall, AddReturn>::new(
        ClientParams::new(participant.clone()).service_name("MultiAdd"),
    )
    .unwrap();

    let echo_client = Client::<EchoCall, EchoReturn>::new(
        ClientParams::new(participant.clone()).service_name("MultiEcho"),
    )
    .unwrap();

    add_client.wait_for_service_timeout(Duration::from_secs(5)).unwrap();
    echo_client.wait_for_service_timeout(Duration::from_secs(5)).unwrap();

    let mut server = Server::new(ServerParams::new());
    server.add_service(add_service);
    server.add_service(echo_service);
    let handle = thread::spawn(move || server.run_for(Duration::from_secs(5)));

    // Add service
    add_client.send_request(&AddCall { a: 5, b: 3 }).unwrap();
    let reply = add_client.receive_reply(Duration::from_secs(5)).unwrap();
    assert_eq!(reply.data().unwrap().data.result, 8);

    // Echo service
    echo_client.send_request(&EchoCall { msg: "hello".to_string() }).unwrap();
    let reply = echo_client.receive_reply(Duration::from_secs(5)).unwrap();
    assert_eq!(reply.data().unwrap().data.msg, "hello");

    handle.join().unwrap().unwrap();
}

#[test]
fn server_run_for_duration() {
    let domain_id = next_domain_id();
    let participant = make_participant(domain_id);

    let service = Service::new(
        ServiceParams::new(participant.clone()).service_name("RunForDuration"),
        AddHandler,
    )
    .unwrap();

    let mut server = Server::new(ServerParams::new());
    server.add_service(service);

    let start = std::time::Instant::now();
    server.run_for(Duration::from_millis(300)).unwrap();
    let elapsed = start.elapsed();

    assert!(elapsed >= Duration::from_millis(250));
    assert!(elapsed < Duration::from_millis(600));
}

#[test]
fn service_pause_resume() {
    let domain_id = next_domain_id();
    let participant = make_participant(domain_id);
    let svc_name = "PauseResume";

    let mut service =
        Service::new(ServiceParams::new(participant.clone()).service_name(svc_name), AddHandler)
            .unwrap();

    assert_eq!(service.status(), ServiceStatus::Running);

    service.pause();
    assert_eq!(service.status(), ServiceStatus::Paused);

    service.resume();
    assert_eq!(service.status(), ServiceStatus::Running);
}

#[test]
fn service_close_via_server() {
    let domain_id = next_domain_id();
    let participant = make_participant(domain_id);

    let service = Service::new(
        ServiceParams::new(participant.clone()).service_name("ServerClose"),
        AddHandler,
    )
    .unwrap();

    let mut server = Server::new(ServerParams::new());
    server.add_service(service);

    assert!(!server.is_closed());
    server.close().unwrap();
    assert!(server.is_closed());
}
