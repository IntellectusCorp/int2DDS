//! RobotControl client example.
//!
//! Start the service first, then run this client.

#[path = "types/mod.rs"]
mod types;

use std::time::Duration;

use int2dds::dcps::domain::domain_participant_factory::DomainParticipantFactory;
use int2dds::dcps::domain::qos::DomainParticipantQos;
use int2dds::dcps::infrastructure::status::StatusMask;

use int2dds_rpc::client::ClientParams;
use int2dds_rpc::entity::ServiceProxy;
use int2dds_rpc::error::DdsRpcError;

use types::*;

fn main() {
    let participant = DomainParticipantFactory::get_instance()
        .create_participant(0, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let client = RobotControlClient::new(ClientParams::new(participant.clone())).unwrap();

    println!("[Client] Waiting for service...");
    client.wait_for_service_timeout(Duration::from_secs(10)).unwrap();
    println!("[Client] Service found!");

    let timeout = Duration::from_secs(5);

    print!("[Client] 1/7 command(StartCommand) ... ");
    client.command(Command::StartCommand, timeout).unwrap();
    println!("OK");

    print!("[Client] 2/7 setSpeed(50.0) ... ");
    let old = client.set_speed(50.0, timeout).unwrap();
    println!("previous speed: {}", old);

    print!("[Client] 3/7 setSpeed(200.0) ... ");
    match client.set_speed(200.0, timeout) {
        Ok(v) => println!("unexpected success: {}", v),
        Err(DdsRpcError::UserException(ex)) => {
            println!("TooFast: max_speed={}", ex.max_speed);
        }
        Err(e) => println!("unexpected error: {:?}", e),
    }

    print!("[Client] 4/7 getSpeed() ... ");
    let speed = client.get_speed(timeout).unwrap();
    println!("speed: {}", speed);

    print!("[Client] 5/7 getStatus() ... ");
    let status = client.get_status(timeout).unwrap();
    println!("msg={}, code={}", status.msg, status.code);

    print!("[Client] 6/7 navigate(10.0, 20.0) ... ");
    client.navigate(10.0, 20.0, timeout).unwrap();
    println!("OK");

    print!("[Client] 7/7 navigate(9999.0, 0.0) ... ");
    match client.navigate(9999.0, 0.0, timeout) {
        Ok(()) => println!("unexpected success"),
        Err(DdsRpcError::UserException(err)) => match err {
            RobotControl_navigate_Error::TooFast(ex) => {
                println!("TooFast: max_speed={}", ex.max_speed);
            }
            RobotControl_navigate_Error::InvalidCommand(ex) => {
                println!("InvalidCommand: reason={}", ex.reason);
            }
        },
        Err(e) => println!("unexpected error: {:?}", e),
    }

    println!("\n[Client] Done.");
}
