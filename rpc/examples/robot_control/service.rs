//! RobotControl service example.
//!
//! Run this first, then start the client in another terminal.

#[path = "types/mod.rs"]
mod types;

use std::sync::Mutex;
use std::time::Duration;

use int2dds::dcps::domain::domain_participant_factory::DomainParticipantFactory;
use int2dds::dcps::domain::qos::DomainParticipantQos;
use int2dds::dcps::infrastructure::status::StatusMask;

use int2dds_rpc::server::{Server, ServerParams};
use int2dds_rpc::service::ServiceParams;

use types::*;

// Service implementation that handles incoming RPC requests
struct RobotControlImpl {
    speed: Mutex<f32>,
}

impl RobotControlImpl {
    fn new() -> Self {
        Self { speed: Mutex::new(0.0) }
    }
}

// Implement the RobotControl trait to define how each RPC call is handled
impl RobotControl for RobotControlImpl {
    fn command(&self, com: Command) {
        print!("[Service] command({:?}) ... ", com);
        println!("OK");
    }

    fn set_speed(&self, speed: f32) -> Result<f32, TooFast> {
        print!("[Service] setSpeed({}) ... ", speed);
        if speed > 100.0 {
            println!("TooFast: max_speed=100");
            return Err(TooFast { max_speed: 100.0 });
        }
        let mut s = self.speed.lock().unwrap();
        let old = *s;
        *s = speed;
        println!("previous speed: {}", old);
        Ok(old)
    }

    fn get_speed(&self) -> f32 {
        print!("[Service] getSpeed() ... ");
        let s = *self.speed.lock().unwrap();
        println!("speed: {}", s);
        s
    }

    fn get_status(&self, status: &mut Status) {
        print!("[Service] getStatus() ... ");
        status.msg = "OK".to_string();
        status.code = 0;
        println!("msg={}, code={}", status.msg, status.code);
    }

    fn navigate(&self, x: f32, y: f32) -> Result<(), RobotControl_navigate_Error> {
        print!("[Service] navigate({}, {}) ... ", x, y);
        if x.abs() > 1000.0 || y.abs() > 1000.0 {
            println!("InvalidCommand: coordinates out of range");
            return Err(RobotControl_navigate_Error::InvalidCommand(InvalidCommand {
                reason: "coordinates out of range".to_string(),
            }));
        }
        println!("OK");
        Ok(())
    }
}

fn main() {
    // Create a DDS domain participant for communication
    let participant = DomainParticipantFactory::get_instance()
        .create_participant(0, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    // Create the RPC service with the implementation
    let service =
        RobotControlService::new(ServiceParams::new(participant.clone()), RobotControlImpl::new())
            .unwrap();

    // Create a server and register the service
    let mut server = Server::new(ServerParams::new());
    server.add_service(service);

    println!("[Service] Running RobotControl service...");
    println!("[Service] Press Ctrl+C to stop.");

    // Run the server and process incoming requests
    server.run().unwrap();
}
