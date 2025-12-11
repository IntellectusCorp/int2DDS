# int2DDS

<div align="center">

A **Rust implementation** of the [Data Distribution Service (DDS)](https://www.omg.org/spec/DDS/) middleware standard, following the Real-Time Publish-Subscribe (RTPS) protocol.

[English](#english) | [한국어](#korean)

</div>

---

<a name="english"></a>

## Overview

**int2DDS** is a production-grade DDS middleware implementation in Rust, developed by Intellectus Corp. It provides high-performance, real-time pub-sub communication for distributed systems with full RTPS protocol support.

### Key Features

- **RTPS Protocol**: Implementation of the OMG RTPS 2.5 wire protocol
- **QoS Policies**: Comprehensive Quality of Service support (Reliability, Durability, History, Liveliness, etc.)
- **Automatic Discovery**: Built-in SPDP and SEDP for automatic endpoint discovery
- **Type Safety**: Compile-time type checking with `#[derive(DdsType)]` macro
- **Transport Flexibility**: UDP multicast, broadcast, and TCP transport support
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

**Publisher:**

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

**Subscriber:**

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
- `INT2DDS_BROADCAST_ENABLED`: Enable broadcast discovery alongside multicast
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

## License

This project is licensed under the [Apache License 2.0](LICENSE).

## Acknowledgments

Developed by [Intellectus Corp](https://github.com/IntellectusCorp).

---

<a name="korean"></a>

## 한국어

### 개요

**int2DDS**는 인텔렉투스에서 개발한 Rust 기반 DDS(Data Distribution Service) 미들웨어 구현체입니다. 실시간 분산 시스템을 위한 고성능 pub-sub 통신을 RTPS 프로토콜로 제공합니다.

### 주요 기능

- **RTPS 프로토콜**: OMG RTPS 2.5 와이어 프로토콜 구현
- **QoS 정책**: 신뢰성, 내구성, 히스토리, 생존성 등 포괄적인 QoS 지원
- **자동 검색**: SPDP 및 SEDP를 통한 자동 엔드포인트 검색
- **타입 안전성**: `#[derive(DdsType)]` 매크로로 컴파일 타임 타입 체킹
- **전송 유연성**: UDP 멀티캐스트, 브로드캐스트, TCP 전송 지원
- **데이터 단편화**: 대용량 메시지 자동 처리
- **XCDR2 직렬화**: 확장 가능한 데이터 표현 지원
- **크로스 플랫폼**: Windows, Linux, macOS 지원
- **상호운용성**: 다른 RTPS/DDS 구현체와 상호운용 가능

### 빠른 시작

설치 및 사용 방법은 위의 영어 섹션을 참고하세요.

### 문서

- **API 문서**: `cargo doc --open` 실행
- **기여 가이드**: [CONTRIBUTING.md](CONTRIBUTING.md)
- **예제**: [dds/examples/](dds/examples/) 디렉토리

### 프로젝트 구조

```
int2DDS/
├── dds/              # DDS 라이브러리 구현
│   ├── src/
│   │   ├── dcps/     # DCPS 레이어 (엔티티, QoS, 토픽)
│   │   ├── rtps/     # RTPS 프로토콜 레이어
│   │   └── common/   # 공통 유틸리티
│   ├── examples/     # 예제 프로그램
│   ├── tests/        # 통합 테스트
│   └── benches/      # 성능 벤치마크
├── derive/           # DdsType 매크로
└── docs/             # 문서 및 가이드
```

### 라이선스

이 프로젝트는 [Apache License 2.0](./LICENSE) 라이선스로 배포됩니다.

### 문의

인텔렉투스 (Intellectus Corp)에서 개발했습니다.
