# int2dds-rpc

DDS-RPC (Remote Procedure Call over DDS) implementation for int2DDS, based on the [OMG DDS-RPC specification](https://www.omg.org/spec/DDS-RPC/).

## Overview

DDS excels at publish-subscribe communication, but many systems also require request-reply interactions. DDS-RPC addresses this by providing a standardized request-reply and function-call abstraction on top of DDS. This enables typed RPC semantics while retaining DDS’s QoS, discovery, and transport benefits.

int2dds-rpc is an implementation of this model in Rust. It provides a function-call style API on top of DDS-RPC. You define service interfaces in OMG IDL, generate Rust code using int2dds-idl, and then use the generated client and service types to perform remote procedure calls over DDS.

> **Note:** Currently only the **Basic Service Mapping** profile is implemented. The Enhanced Service Mapping profile is not yet supported.

**Workflow:**

1. Define your interface in an `.idl` file
2. Generate Rust types with `int2dds-idl --rpc`
3. Implement the generated service trait
4. Use the generated client to call remote methods

## Quick Start: Robot Control Example

### Step 1 — Define the IDL

Create `robot_control.idl`:

```idl
module robot {
    enum Command { START_COMMAND, STOP_COMMAND };

    struct Status {
        string msg;
        long code;
    };

    exception TooFast {
        float max_speed;
    };

    exception InvalidCommand {
        string reason;
    };

    interface RobotControl {
        void command(in Command com);
        float setSpeed(in float speed) raises (TooFast);
        float getSpeed();
        void getStatus(out Status status);
        void navigate(in float x, in float y) raises (TooFast, InvalidCommand);
    };
};
```

### Step 2 — Generate Rust Code

```bash
int2dds-idl --rpc types.rs robot_control.idl
```

This generates all the types needed for RPC communication:

| Generated Item | Description |
|---|---|
| `Command`, `Status`, `TooFast`, `InvalidCommand` | IDL types mapped to Rust |
| `RobotControl` trait | Service interface to implement |
| `RobotControlClient` | Type-safe client with a method per IDL operation |
| `RobotControlService` | Service wrapper that dispatches requests to your implementation |
| `RobotControl_navigate_Error` | Enum for methods that raise multiple exceptions |

### Step 3 — Implement the Service

```rust
use int2dds::dcps::domain::domain_participant_factory::DomainParticipantFactory;
use int2dds::dcps::domain::qos::DomainParticipantQos;
use int2dds::dcps::infrastructure::status::StatusMask;

use int2dds_rpc::server::{Server, ServerParams};
use int2dds_rpc::service::ServiceParams;

struct RobotControlImpl {
    speed: Mutex<f32>,
}

impl RobotControl for RobotControlImpl {
    fn command(&self, com: Command) {
        println!("Received command: {:?}", com);
    }

    fn set_speed(&self, speed: f32) -> Result<f32, TooFast> {
        if speed > 100.0 {
            return Err(TooFast { max_speed: 100.0 });
        }
        let mut s = self.speed.lock().unwrap();
        let old = *s;
        *s = speed;
        Ok(old)
    }

    fn get_speed(&self) -> f32 {
        *self.speed.lock().unwrap()
    }

    fn get_status(&self, status: &mut Status) {
        status.msg = "OK".to_string();
        status.code = 0;
    }

    fn navigate(&self, x: f32, y: f32) -> Result<(), RobotControl_navigate_Error> {
        if x.abs() > 1000.0 || y.abs() > 1000.0 {
            return Err(RobotControl_navigate_Error::InvalidCommand(
                InvalidCommand { reason: "coordinates out of range".to_string() },
            ));
        }
        Ok(())
    }
}

fn main() {
    let participant = DomainParticipantFactory::get_instance()
        .create_participant(0, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let service = RobotControlService::new(
        ServiceParams::new(participant.clone()),
        RobotControlImpl { speed: Mutex::new(0.0) },
    ).unwrap();

    let mut server = Server::new(ServerParams::new());
    server.add_service(service);
    server.run().unwrap(); // blocks, dispatching requests
}
```

`Server` can host multiple services — just call `add_service()` for each one.

### Step 4 — Use the Client

```rust
use int2dds_rpc::client::ClientParams;
use int2dds_rpc::entity::ServiceProxy;
use int2dds_rpc::error::DdsRpcError;

fn main() {
    let participant = DomainParticipantFactory::get_instance()
        .create_participant(0, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let client = RobotControlClient::new(ClientParams::new(participant.clone())).unwrap();

    // Wait for the service to be discovered
    client.wait_for_service_timeout(Duration::from_secs(10)).unwrap();

    let timeout = Duration::from_secs(5);

    // Simple call
    client.command(Command::StartCommand, timeout).unwrap();

    // Call with return value
    let prev_speed = client.set_speed(50.0, timeout).unwrap();
    println!("Previous speed: {}", prev_speed);

    // Call with out parameter (returned as value)
    let status = client.get_status(timeout).unwrap();
    println!("Status: {} (code={})", status.msg, status.code);

    // Handling a single exception
    match client.set_speed(200.0, timeout) {
        Ok(v) => println!("Speed set: {}", v),
        Err(DdsRpcError::UserException(ex)) => {
            println!("TooFast! max_speed={}", ex.max_speed);
        }
        Err(e) => eprintln!("Error: {:?}", e),
    }

    // Handling multiple exceptions
    match client.navigate(9999.0, 0.0, timeout) {
        Ok(()) => {}
        Err(DdsRpcError::UserException(err)) => match err {
            RobotControl_navigate_Error::TooFast(ex) => {
                println!("TooFast! max_speed={}", ex.max_speed);
            }
            RobotControl_navigate_Error::InvalidCommand(ex) => {
                println!("Invalid: {}", ex.reason);
            }
        },
        Err(e) => eprintln!("Error: {:?}", e),
    }
}
```

## Running the Example

```bash
# Terminal 1 — start the service
cargo run --example robot_control_service

# Terminal 2 — run the client
cargo run --example robot_control_client
```

## Error Handling

All client methods return `DdsRpcResult<T, E>`, which resolves to `Result<T, DdsRpcError<E>>`:

```rust
match client.some_method(args, timeout) {
    Ok(value) => { /* success */ }
    Err(DdsRpcError::UserException(ex))  => { /* IDL-defined exception */ }
    Err(DdsRpcError::Remote(code))       => { /* RemoteExceptionCode (e.g. UnknownOperation) */ }
    Err(DdsRpcError::Timeout)            => { /* no reply within timeout */ }
    Err(DdsRpcError::Dds(err))           => { /* underlying DDS error */ }
}
```

## Configuration

Both `ClientParams` and `ServiceParams` support builder-style configuration:

```rust
let client = RobotControlClient::new(
    ClientParams::new(participant)
        .service_name("my_robot")
        .instance_name("robot_1"),
).unwrap();

let service = RobotControlService::new(
    ServiceParams::new(participant)
        .service_name("my_robot")
        .instance_name("robot_1"),
    MyImpl::new(),
).unwrap();
```

## Architecture

```
┌──────────────────────────────────────────────────────┐
│                   IDL Definition                     │
│              (robot_control.idl)                     │
└──────────────────────┬───────────────────────────────┘
                       │  int2dds-idl --rpc
                       ▼
┌──────────────────────────────────────────────────────┐
│                Generated Code (types.rs)             │
│  Traits, Client, Service, Call/Return enums, etc.    │
└───────┬──────────────────────────────────┬───────────┘
        │                                  │
        ▼                                  ▼
┌───────────────┐                 ┌────────────────┐
│ Client Side   │    DDS Topics   │ Service Side   │
│               │ ◄─────────────► │                │
│ RobotControl- │  Request/Reply  │ RobotControl-  │
│ Client        │                 │ Service        │
│               │                 │   + Server     │
└───────────────┘                 └────────────────┘
```
