# int2dds

A Rust implementation of the OMG [Data Distribution Service (DDS)](https://www.omg.org/spec/DDS/)
middleware standard with RTPS protocol support.

int2dds provides real-time publish-subscribe communication for distributed systems in domains such as
autonomous driving, robotics, industrial automation, and defense.

## Features

- RTPS 2.5 wire protocol
- QoS policies (Reliability, Durability, History, Liveliness, etc.)
- Automatic discovery with SPDP and SEDP
- Topic types defined with `#[derive(DdsType)]`
- UDP multicast and TCP transports
- Fragmentation of large samples
- XCDR1 and XCDR2 serialization
- DDS-XTypes: TypeObject discovery, type assignability checking, and dynamic types
- Windows, Linux, and macOS
- Interoperable with other RTPS/DDS implementations

## Installation

```toml
[dependencies]
int2dds = "1.0.0"
speedy = "0.8"
log = "0.4"
```

## Usage

Publisher:

```rust,ignore
use int2dds::{
    common::instance_handle::InstanceHandle,
    domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
    infrastructure::status::StatusMask,
    publication::qos::{DataWriterQos, PublisherQos},
    topic::{qos::TopicQos, type_support::DdsType},
};

#[derive(DdsType)]
struct HelloWorld {
    index: u32,
    message: String,
}

fn main() {
    let participant = DomainParticipantFactory::get_instance()
        .create_participant(0, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();
    let topic = participant
        .create_topic::<HelloWorld>("HelloWorldTopic", "HelloWorld", TopicQos::default(), None, StatusMask::default())
        .unwrap();
    let publisher = participant
        .create_publisher(PublisherQos::default(), None, StatusMask::default())
        .unwrap();
    let writer = publisher
        .create_datawriter::<HelloWorld>(&topic, DataWriterQos::default(), None, StatusMask::default())
        .unwrap();

    for index in 0.. {
        let sample = HelloWorld { index, message: format!("Hello, DDS! #{index}") };
        writer.write(&sample, InstanceHandle::NIL).unwrap();
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
```

Subscriber:

```rust,ignore
use std::sync::Arc;

use int2dds::{
    domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
    infrastructure::status::StatusMask,
    subscription::{
        data_reader::DataReader,
        data_reader_listener::DataReaderListener,
        qos::{DataReaderQos, SubscriberQos},
        sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
    },
    topic::{qos::TopicQos, type_support::DdsType},
};

#[derive(DdsType)]
struct HelloWorld {
    index: u32,
    message: String,
}

struct HelloWorldListener;

impl DataReaderListener for HelloWorldListener {
    type Foo = HelloWorld;

    fn on_data_available(&self, reader: &DataReader<Self::Foo>) {
        let Ok(samples) = reader.take(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        ) else {
            return;
        };

        for sample in samples.iter() {
            if let Ok(data) = sample.data() {
                println!("Received: {data:?}");
            }
        }
    }
}

fn main() {
    let participant = DomainParticipantFactory::get_instance()
        .create_participant(0, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();
    let topic = participant
        .create_topic::<HelloWorld>("HelloWorldTopic", "HelloWorld", TopicQos::default(), None, StatusMask::default())
        .unwrap();
    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();
    let _reader = subscriber
        .create_datareader::<HelloWorld>(
            &topic,
            DataReaderQos::default(),
            Some(Arc::new(HelloWorldListener)),
            StatusMask::default(),
        )
        .unwrap();

    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
```

For environment variables, C#/Python/Java bindings, and more examples, see the
[int2DDS repository](https://github.com/IntellectusCorp/int2DDS).

## License

Licensed under the [Apache License 2.0](https://github.com/IntellectusCorp/int2DDS/blob/main/LICENSE).
