# int2DDS

<div align="center">

A **Rust implementation** of the [Data Distribution Service (DDS)](https://www.omg.org/spec/DDS/) middleware standard,
following the Real-Time Publish-Subscribe (RTPS)

</div>

---

## Overview

**int2DDS** is an open-source, real-time DDS / RTPS middleware core. It provides high-performance, real-time pub-sub communication for distributed systems with RTPS protocol support. It provides a standards-based foundation for building reliable, low-latency distributed systems in domains such as autonomous driving, robotics, industrial automation, and defense.

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
int2dds = "0.0.1"
int2dds-derive = "0.0.1"
```

### Basic Example

<details>
<summary><strong>Publisher:</strong></summary>

```rust,ignore
use int2dds::{
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

// Define custom DDS type with #[derive(DdsType)]
// Use #[dds(key)] attribute to mark key fields
#[derive(DdsType)]
#[dds_type(crate_path = "int2dds")]
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

```rust,ignore
use std::sync::Arc;
use int2dds::{
    core::time::Duration,
    domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
    infrastructure::{
        qos_policy::{ReliabilityQosPolicy, ReliabilityQosPolicyKind},
        status::StatusMask,
    },
    subscription::{
        data_reader_listener::DataReaderListener,
        qos::{DataReaderQos, SubscriberQos},
        sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
    },
    topic::{qos::TopicQos, type_support::DdsType},
};

// Define custom DDS type with #[derive(DdsType)]
// Use #[dds(key)] attribute to mark key fields
#[derive(DdsType)]
#[dds_type(crate_path = "int2dds")]
struct HelloWorld {
    index: u32,
    message: String,
}

struct MyListener;
impl DataReaderListener for MyListener {
    type Foo = HelloWorld;
    fn on_data_available(
        &self,
        reader: &int2dds::subscription::data_reader::DataReader<Self::Foo>,
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

**Common (all builds):**

- Rust 1.89 or later, with Cargo
- Git

**Per-language (only if building that binding):**

- C#: .NET SDK 8.0 or later (covers all multi-targets including legacy net45/net48 via reference assemblies)
- Python: Python 3.10 or later, `pip`
- Java: JDK 11 or later (Gradle wrapper is included — no separate install needed)

```bash
git clone https://github.com/IntellectusCorp/int2DDS.git
cd int2DDS
```

### 1. Build the FFI native libraries

The C# and Python bindings load `int2dds-ffi` (a `cdylib`) at runtime. The Java binding uses a separate JNI crate (`int2dds-java`).

```bash
# Required for C# and Python bindings
cargo build --release -p int2dds-ffi

# Required for the Java binding (currently on the feature/java-binding branch)
cargo build --release -p int2dds-java
```

Outputs land in `target/release/`:

- `int2dds_ffi.{dll,so,dylib}` — for C# / Python
- `int2dds_java.{dll,so,dylib}` — for Java

> **Note:** native libraries are built for the host architecture by default. The process loading them must match (e.g. an x64 .dll cannot be loaded into a 32-bit process). Cross-compile with `cargo build --release --target <triple>` if needed.

### 2. Rust core

```bash
cargo build --release
cargo test
```

### 3. C# binding

```bash
dotnet build csharp/Int2Dds.sln -c Release
dotnet test csharp/tests/Int2Dds.Tests/Int2Dds.Tests.csproj -c Release
```

The C# build does **not** auto-copy the native FFI dll. Copy it next to the executable before running an example:

```bash
# Windows; adjust the TFM (net8.0, net6.0, …) as needed
cp target/release/int2dds_ffi.dll csharp/examples/HelloWorldPub/bin/Release/net8.0/
```

> Run from an ASCII path — the FFI loader currently fails silently on paths containing non-ASCII characters (e.g. Korean).

See [csharp/](csharp/) for projects and examples.

### 4. Python binding

```bash
cd python
pip install -e .
pytest
```

The wrapper auto-discovers `target/release/int2dds_ffi.{dll,so,dylib}` from the workspace root. If the library lives elsewhere, set:

```bash
export INT2DDS_FFI_PATH=/abs/path/to/int2dds_ffi.dll
```

See [python/README.md](python/README.md) for the full guide.

### 5. Java binding

```bash
cd java
./gradlew build
./gradlew test
```

The native loader looks in this order: the `INT2DDS_JAVA_LIB` environment variable → bundled JAR resources → `java.library.path`. For local development:

```bash
export INT2DDS_JAVA_LIB=/abs/path/to/target/release/int2dds_java.dll
```

> **Status:** the Java JNI crate lives on the `feature/java-binding` branch and is being integrated; the API module under [java/int2dds-api/](java/int2dds-api/) compiles independently.

### Running Examples

```bash
# Rust
cargo run --release --example hello_world_pub -- --domain 0
cargo run --release --example hello_world_sub -- --domain 0

# C# (requires int2dds_ffi.dll copied next to the exe; see step 3)
dotnet run -c Release --project csharp/examples/HelloWorldPub -f net8.0
dotnet run -c Release --project csharp/examples/HelloWorldSub -f net8.0

# Python
python python/examples/hello_world_pub.py
python python/examples/hello_world_sub.py
```

> These are the minimal per-language hello_world smoke examples that ship in the core repo.

## Documentation

- **API Documentation**: Run `cargo doc --open --no-deps` to generate and view API docs
- **Contributing**: See [CONTRIBUTING.md](https://github.com/IntellectusCorp/int2DDS/blob/main/CONTRIBUTING.md) for contribution guidelines
- **Examples**: The core repo ships a minimal per-language hello_world; the comprehensive, topic-organized examples live in the separate [int2DDS-examples](https://github.com/IntellectusCorp/int2DDS-examples) repository

## Environment Variables

int2DDS supports various environment variables for configuration:

### Basic Environment Variables

- `INT2DDS_TRANSPORT`: Transport protocol (udp, tcp, hybrid, shm)
- `INT2DDS_LOG_TYPE`: Log output type (console, file, all, none)
- `INT2DDS_CONSOLE_LOG_LEVEL`: Console log level (error, warn, info, debug, trace)
- `INT2DDS_FILE_LOG_LEVEL`: File log level (error, warn, info, debug, trace)
- `INT2DDS_UDP_SOCKET_BUFFER`: UDP socket buffer size (bytes), increase up to 8388608(8MB) for large payloads
- `INT2DDS_USE_LOOPBACK_INTERFACE`: Enable loopback interface for endpoint communication
- `INT2DDS_FORCE_LOOPBACK_MULTICAST`: Force multicast egress through the loopback interface (127.0.0.1) for local-only testing, use together with `INT2DDS_USE_LOOPBACK_INTERFACE`
- `INT2DDS_MULTICAST_TTL`: IPv4 multicast TTL fallback (0-255), used when `PropertyQosPolicy` has no `int2dds.transport.UDPv4.multicast_ttl` entry (default: 1)
- `INT2DDS_INITIAL_PEERS`: Initial peer list for unicast (format: "ip:port,ip:port,...")

### int2DDS-feature dependent Environment Variables

These variables require [int2DDS-feature](https://github.com/IntellectusCorp/int2DDS-feature-releases) binary. Place the binary in the same directory as your executable.

- `INT2DDS_NETWORK_INTERFACE`: Specify network interface (e.g., "eth0", "Ethernet")
- `INT2DDS_NETWORK_IP`: Specify network IP address directly
- `INT2DDS_EXTENDED_DISCOVERY`: Enable extended discovery alongside multicast

### Performance Monitoring Environment Variables

- `INT2DDS_THREAD_MONITORING`: Enable thread monitoring (default: false)
- `INT2DDS_THREAD_MONITORING_LOG_PATH`: Thread monitoring log file path (default: ./thread_monitoring.log)
- `INT2DDS_FUNCTION_TIMING`: Enable function execution time measurement (default: false)
- `INT2DDS_FUNCTION_TIMING_LOG_PATH`: Function timing log file path (default: ./function_timing.log)

### Additional Environment Variables

- For detailed environment variable documentation, see [docs/guide/env.md](https://github.com/IntellectusCorp/int2DDS/blob/main/docs/guide/env.md).

## Project Structure

```text
int2DDS/
├── dds/              # Main DDS library implementation
│   ├── src/
│   │   ├── dcps/     # DCPS layer (entities, QoS, topics)
│   │   ├── rtps/     # RTPS protocol layer
│   │   └── common/   # Shared utilities
│   ├── examples/     # Minimal hello_world examples
│   ├── tests/        # Integration tests
│   └── benches/      # Performance benchmarks
├── derive/           # DdsType derive macro
└── docs/             # Documentation and guides
```

## Contributing

We welcome contributions! Please see [CONTRIBUTING.md](https://github.com/IntellectusCorp/int2DDS/blob/main/CONTRIBUTING.md) for details on:

- Setting up your development environment
- Code style guidelines
- Submitting pull requests
- Reporting issues

All contributors must agree to a **Contributor License Agreement (CLA)** before contributions can be merged.

- Individual contributors: [`CLA-Individual.md`](https://github.com/IntellectusCorp/int2DDS/blob/main/CLA-Individual.md)
- Corporate / organizational contributors: [`CLA-Corporate.md`](https://github.com/IntellectusCorp/int2DDS/blob/main/CLA-Corporate.md)

## Scope of int2DDS

int2DDS focuses on the **core runtime and communication layer** of a DDS middleware implementation.
It is designed to be embedded and reused by higher-level systems, including commercial products such as **int2ConneX**, without imposing strong copyleft obligations.

## Scope of the Open-Source Core

### Included

- OMG DDS / RTPS standard-based core functionality
- Discovery, transport, topics, and essential QoS mechanisms
- Minimal tooling required to build, run, and test the middleware

### Not Included

- Centralized management or monitoring dashboards
- Fleet or cluster orchestration
- Domain-specific bridges, UIs, analytics, or data pipelines

These advanced capabilities are provided through commercial products such as **int2ConneX**.

## License

This project is licensed under the [Apache License 2.0](https://github.com/IntellectusCorp/int2DDS/blob/main/LICENSE).

## Acknowledgments

Developed by [Intellectus Corp](https://github.com/IntellectusCorp).
