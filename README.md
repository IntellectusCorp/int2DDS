# int2DDS

<div align="center">

A **Rust implementation** of the [Data Distribution Service (DDS)](https://www.omg.org/spec/DDS/) middleware standard, following the Real-Time Publish-Subscribe (RTPS)

</div>

---

## Overview

**int2DDS** is a production-grade DDS middleware implementation in Rust, developed by Intellectus Corp. It provides high-performance, real-time pub-sub communication for distributed systems with full RTPS protocol support.

### Key Features

- **RTPS Protocol**: Implementation of the OMG RTPS 2.5 wire protocol
- **QoS Policies**: Comprehensive Quality of Service support (Reliability, Durability, History, Liveliness, etc.)
- **Automatic Discovery**: Built-in SPDP and SEDP for automatic endpoint discovery
- **Type Safety**: Compile-time type checking with `#[derive(DdsType)]` macro
- **Transport Flexibility**: UDP multicast, and TCP transport support
- **Data Fragmentation**: Automatic handling of large messages
- **XCDR2 Serialization**: Support for extensible data representation
- **Cross-Platform**: Works on Windows, Linux, and macOS
- **Interoperability**: Interoperable with other RTPS/DDS implementations

## Quick Start

### Installation

Add int2DDS to your `Cargo.toml`:

```toml
[dependencies]
int2DDS = "0.0.1"
int2DDS-derive = "0.0.1"
```

### Basic Example

<details>
<summary><strong>Publisher:</strong></summary>

```rust
use int2DDS::{
    common::instance_handle::InstanceHandle,
    core::time::Duration,
    domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
    infrastructure::{
        qos_policy::{ReliabilityQosPolicy, ReliabilityQosPolicyKind},
        status::StatusMask,
    },
    publication::{
        qos::{DataWriterQos, PublisherQos},
    },
    topic::{qos::TopicQos, type_support::DdsType},
};


#[derive(DdsType)]
#[dds_type(crate_path = "int2DDS")]
struct HelloWorld {
    index: u32,
    message: String,
}

fn main() {
    let domain_id = 0;

    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let topic = participant
        .create_topic::<HelloWorld>(
            "HelloWorldTopic",
            "HelloWorld",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let publisher = participant
        .create_publisher(PublisherQos::default(), None, StatusMask::default())
        .unwrap();
    let writer_qos = DataWriterQos::default();
    let writer = publisher
        .create_datawriter::<HelloWorld>(
            &topic,
            writer_qos,
            None,
            StatusMask::default(),
        )
        .unwrap();

    let mut i = 0;
    loop {
        let data = HelloWorld {
            index: i,
            message: format!("Hello, DDS! #{}", i),
        };
        writer.write(&data, InstanceHandle::NIL).unwrap();
        std::thread::sleep(std::time::Duration::from_secs(1));
        i += 1;
    }
}
```

</details>

<details>
<summary><strong>Subscriber:</strong></summary>

```rust
use int2DDS::{
    core::time::Duration,
    domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
    infrastructure::{
        qos_policy::{ReliabilityQosPolicy, ReliabilityQosPolicyKind},
        status::StatusMask,
    },
    subscription::{
        qos::{DataReaderQos, SubscriberQos},
        sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
    },
    topic::{qos::TopicQos, type_support::DdsType},
};

#[derive(DdsType)]
#[dds_type(crate_path = "int2DDS")]
struct HelloWorld {
    index: u32,
    message: String,
}

struct MyListener;
impl DataReaderListener for MyListener {
    type Foo = HelloWorld;
    fn on_data_available(
        &self,
        reader: &int2DDS::subscription::data_reader::DataReader<Self::Foo>,
    ) {
        if let Ok(samples) = reader.take(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        ) {
            for sample in samples.iter() {
                if let Ok(data) = sample.data() {
                    println!("Received: {:?}", data);
                }
            }
        }
    }
}

fn main() {
    let domain_id = 0;

    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let topic = participant
        .create_topic::<HelloWorld>(
            "HelloWorldTopic",
            "HelloWorld",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();
    let reader_qos = DataReaderQos {
        reliability: ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::Reliable,
            max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
        },
        ..Default::default()
    };
    let _reader = subscriber
        .create_datareader::<HelloWorld>(
            &topic,
            reader_qos,
            Some(Arc::new(MyListener)),
            StatusMask::default(),
        )
        .unwrap();

    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
```

</details>

## Building from Source

### Prerequisites

- Rust 1.70 or later
- Cargo

### Build

```bash
# Clone the repository
git clone https://github.com/IntellectusCorp/int2DDS.git
cd int2DDS

# Build the project
cargo build

# Run tests
cargo test

# Build with release optimizations
cargo build --release
```

### Running Examples

examples:

```bash
# Basic hello world examples
cargo run --example hello_world_param -- --role pub --domain 0 --reliability reliable
cargo run --example hello_world_param -- --role sub --domain 0 --reliability reliable

# Performance testing
cargo run --example perftest_publisher
cargo run --example perftest_subscriber
```

## Documentation

- **API Documentation**: Run `cargo doc --open` to generate and view API docs
- **Contributing**: See [CONTRIBUTING.md](CONTRIBUTING.md) for contribution guidelines
- **Examples**: Check the [dds/examples/](dds/examples/) directory for comprehensive examples

## Environment Variables

int2DDS supports various environment variables for configuration:

- `INT2DDS_NETWORK_INTERFACE`: Specify network interface (e.g., "eth0")
- `INT2DDS_NETWORK_IP`: Specify network IP address directly
- `INT2DDS_EXTENDED_DISCOVERY`: Enable extended discovery alongside multicast
- `INT2DDS_THREAD_MONITORING`: Enable thread monitoring and logging
- `INT2DDS_FUNCTION_TIMING`: Enable function performance profiling
- `RUST_LOG`: Control logging level (error, warn, info, debug, trace)

For environment variable documentation and advanced configuration, see the documentation in the repository.

## Project Structure

```
int2DDS/
├── dds/              # Main DDS library implementation
│   ├── src/
│   │   ├── dcps/     # DCPS layer (entities, QoS, topics)
│   │   ├── rtps/     # RTPS protocol layer
│   │   └── common/   # Shared utilities
│   ├── examples/     # Example programs
│   ├── tests/        # Integration tests
│   └── benches/      # Performance benchmarks
├── derive/           # DdsType derive macro
└── docs/             # Documentation and guides
```

## Contributing

We welcome contributions! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for details on:

- Setting up your development environment
- Code style guidelines
- Submitting pull requests
- Reporting issues

All contributors must agree to a **Contributor License Agreement (CLA)** before contributions can be merged.

- Individual contributors: [`CLA-Individual.md`](./CLA-Individual.md)
- Corporate / organizational contributors: [`CLA-Corporate.md`](./CLA-Corporate.md)

## License

This project is licensed under the [Apache License 2.0](LICENSE).

## Acknowledgments

Developed by [Intellectus Corp](https://github.com/IntellectusCorp).
