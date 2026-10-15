mod common;

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use std::time::{Duration as StdDuration, Instant};

use common::*;
use int2dds::common::instance_handle::InstanceHandle;
use int2dds::config::xml::XmlTypeRegistry;
use int2dds::dcps::domain::domain_participant::DomainParticipant;
use int2dds::dcps::publication::data_writer::DataWriter;
use int2dds::dcps::subscription::data_reader::DataReader;
use int2dds::dcps::topic::{topic::Topic, topic_listener::TopicListener};
use int2dds::dcps::{
    core::time::Duration,
    domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
    infrastructure::status::{InconsistentTopicStatus, StatusMask},
    publication::qos::{DataWriterQos, PublisherQos},
    subscription::{
        qos::{DataReaderQos, SubscriberQos},
        sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
    },
};
use int2dds::{
    topic::qos::TopicQos,
    xtypes::{DynamicData, DynamicTypeSupport},
};

const TOPIC: &str = "TypeMatchTopic";

// Each pair differs in type name and in bound only.
const BOUNDS_XML: &str = r#"<types>
      <struct name="int32x10" extensibility="final">
        <member name="x1" type="int32" arrayDimensions="10"/>
      </struct>
      <struct name="int32x20" extensibility="final">
        <member name="x1" type="int32" arrayDimensions="20"/>
      </struct>
      <struct name="string10" extensibility="final">
        <member name="x1" type="string" stringMaxLength="10"/>
      </struct>
      <struct name="string20" extensibility="final">
        <member name="x1" type="string" stringMaxLength="20"/>
      </struct>
    </types>"#;

#[derive(Default)]
struct InconsistentTopicCounter {
    calls: AtomicI32,
}

impl TopicListener for InconsistentTopicCounter {
    fn on_inconsistent_topic(&self, _topic: &Topic, _status: &InconsistentTopicStatus) {
        self.calls.fetch_add(1, Ordering::SeqCst);
    }
}

struct Side {
    participant: DomainParticipant,
    topic: Topic,
    listener: Arc<InconsistentTopicCounter>,
}

impl Side {
    fn inconsistent_count(&self) -> i32 {
        self.topic.get_inconsistent_topic_status().unwrap().total_count()
    }

    fn wait_inconsistent(&self, timeout: StdDuration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if self.inconsistent_count() > 0 {
                return true;
            }
            std::thread::sleep(StdDuration::from_millis(100));
        }
        false
    }

    fn delete(self) {
        self.participant.delete_contained_entities().unwrap();
        DomainParticipantFactory::get_instance().delete_participant(self.participant).unwrap();
    }
}

fn support(xml: &str, type_name: &str) -> Arc<DynamicTypeSupport> {
    let mut registry = XmlTypeRegistry::new();
    registry.load_str(xml).unwrap();
    Arc::new(registry.get(type_name).unwrap())
}

fn side(domain_id: i32, support: &Arc<DynamicTypeSupport>) -> Side {
    let participant = DomainParticipantFactory::get_instance()
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();
    let listener = Arc::new(InconsistentTopicCounter::default());
    let topic = participant
        .create_topic_dynamic(
            TOPIC,
            support.clone(),
            TopicQos::default(),
            Some(listener.clone()),
            StatusMask::INCONSISTENT_TOPIC,
        )
        .unwrap();
    Side { participant, topic, listener }
}

// One participant per side so matching goes through SEDP and TypeLookup.
fn writer_reader_pair(
    writer_support: &Arc<DynamicTypeSupport>,
    reader_support: &Arc<DynamicTypeSupport>,
) -> (Side, DataWriter<DynamicData>, Side, DataReader<DynamicData>) {
    let domain_id = next_domain_id();
    let pub_side = side(domain_id, writer_support);
    let publisher = pub_side
        .participant
        .create_publisher(PublisherQos::default(), None, StatusMask::default())
        .unwrap();
    let writer = publisher
        .create_datawriter_dynamic(
            &pub_side.topic,
            writer_support.clone(),
            DataWriterQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let sub_side = side(domain_id, reader_support);
    let subscriber = sub_side
        .participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();
    let reader = subscriber
        .create_datareader_dynamic(
            &sub_side.topic,
            reader_support.clone(),
            DataReaderQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    (pub_side, writer, sub_side, reader)
}

#[test]
fn different_type_names_match_when_assignable() {
    let writer_support = support(BOUNDS_XML, "string10");
    let reader_support = support(BOUNDS_XML, "string20");
    let (pub_side, writer, sub_side, reader) = writer_reader_pair(&writer_support, &reader_support);

    wait_for_writer_status(&writer, StatusMask::PUBLICATION_MATCHED, Duration::from_seconds(10))
        .expect("string<10> must be assignable to string<20> regardless of type name");
    wait_for_reader_status(&reader, StatusMask::SUBSCRIPTION_MATCHED, Duration::from_seconds(10))
        .unwrap();

    let mut data = writer_support.create_data();
    data.set("x1", "ten chars!").unwrap();
    writer.write(&data, InstanceHandle::NIL).unwrap();

    wait_for_reader_status(&reader, StatusMask::DATA_AVAILABLE, Duration::from_seconds(5)).unwrap();
    let samples = reader
        .take(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        )
        .unwrap();
    assert_eq!(samples.len(), 1);
    let received: String = samples[0].data().unwrap().get("x1").unwrap();
    assert_eq!(received, "ten chars!");

    assert_eq!(pub_side.inconsistent_count(), 0);
    assert_eq!(sub_side.inconsistent_count(), 0);

    pub_side.delete();
    sub_side.delete();
}

#[test]
fn different_type_names_incompatible_types_raise_inconsistent_topic_once() {
    let writer_support = support(BOUNDS_XML, "int32x10");
    let reader_support = support(BOUNDS_XML, "int32x20");
    let started = Instant::now();
    let (pub_side, writer, sub_side, reader) = writer_reader_pair(&writer_support, &reader_support);

    assert!(pub_side.wait_inconsistent(StdDuration::from_secs(10)));
    assert!(sub_side.wait_inconsistent(StdDuration::from_secs(10)));
    assert!(
        wait_for_writer_status(&writer, StatusMask::PUBLICATION_MATCHED, Duration::from_seconds(2))
            .is_err(),
        "int32[10] must not match int32[20]"
    );
    assert!(wait_for_reader_status(
        &reader,
        StatusMask::SUBSCRIPTION_MATCHED,
        Duration::from_seconds(1)
    )
    .is_err());

    // The deferred-match timer (5 s) re-evaluates the pair; it must not be reported again.
    let timer_passed = started + StdDuration::from_secs(7);
    if let Some(remaining) = timer_passed.checked_duration_since(Instant::now()) {
        std::thread::sleep(remaining);
    }
    assert_eq!(pub_side.inconsistent_count(), 1);
    assert_eq!(sub_side.inconsistent_count(), 1);
    assert_eq!(pub_side.listener.calls.load(Ordering::SeqCst), 1);
    assert_eq!(sub_side.listener.calls.load(Ordering::SeqCst), 1);

    pub_side.delete();
    sub_side.delete();
}

#[test]
fn inconsistent_topic_counts_remote_endpoint_once_across_local_readers() {
    let writer_support = support(BOUNDS_XML, "int32x10");
    let reader_support = support(BOUNDS_XML, "int32x20");
    let (pub_side, writer, sub_side, reader) = writer_reader_pair(&writer_support, &reader_support);
    let second_reader = sub_side
        .participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap()
        .create_datareader_dynamic(
            &sub_side.topic,
            reader_support.clone(),
            DataReaderQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    // Both local readers must have rejected the remote writer before counting.
    let deadline = Instant::now() + StdDuration::from_secs(10);
    let rejected = |r: &DataReader<DynamicData>| {
        r.get_requested_incompatible_type_status().unwrap().total_count() > 0
    };
    while !(rejected(&reader) && rejected(&second_reader)) {
        assert!(Instant::now() < deadline, "both readers must reject int32[10]");
        std::thread::sleep(StdDuration::from_millis(100));
    }
    assert!(wait_for_writer_status(
        &writer,
        StatusMask::PUBLICATION_MATCHED,
        Duration::from_seconds(1)
    )
    .is_err());
    assert_eq!(sub_side.inconsistent_count(), 1);
    assert_eq!(sub_side.listener.calls.load(Ordering::SeqCst), 1);
    // Two remote readers on the publisher side are two remote endpoints.
    assert_eq!(pub_side.inconsistent_count(), 2);

    pub_side.delete();
    sub_side.delete();
}

const KEY_XML: &str = r#"<types>
      <enum name="E1">
        <enumerator name="VAL0"/><enumerator name="VAL1"/><enumerator name="VAL2"/>
      </enum>
      <enum name="E2">
        <enumerator name="VAL0"/><enumerator name="VAL1"/><enumerator name="VAL2"/>
        <enumerator name="VAL3"/>
      </enum>
      <struct name="key_enum_1">
        <member name="x1" type="nonBasic" nonBasicTypeName="E1" key="true"/>
      </struct>
      <struct name="key_enum_2">
        <member name="x1" type="nonBasic" nonBasicTypeName="E2" key="true"/>
      </struct>
      <struct name="key_string10">
        <member name="x1" type="string" stringMaxLength="10" key="true"/>
      </struct>
      <struct name="key_string20">
        <member name="x1" type="string" stringMaxLength="20" key="true"/>
      </struct>
    </types>"#;

fn key_types_match(writer_type: &str, reader_type: &str) -> bool {
    let writer_support = support(KEY_XML, writer_type);
    let reader_support = support(KEY_XML, reader_type);
    let (pub_side, writer, sub_side, _reader) =
        writer_reader_pair(&writer_support, &reader_support);
    let deadline = Instant::now() + StdDuration::from_secs(10);
    let matched = loop {
        if writer.get_publication_matched_status().unwrap().current_count() > 0 {
            break true;
        }
        if sub_side.inconsistent_count() > 0 {
            break false;
        }
        assert!(Instant::now() < deadline, "{writer_type} -> {reader_type}: no decision");
        std::thread::sleep(StdDuration::from_millis(100));
    };
    pub_side.delete();
    sub_side.delete();
    matched
}

// One test so the env switch cannot leak into a concurrently running case.
#[test]
fn key_member_rules_follow_rti_key_compat_env() {
    // XTypes 1.3 Table 19: the writer key must map onto the reader key.
    assert!(key_types_match("key_enum_1", "key_enum_2"));
    assert!(!key_types_match("key_enum_2", "key_enum_1"), "writer key literal VAL3 is unmapped");
    assert!(
        !key_types_match("key_string20", "key_string10"),
        "key bound is checked despite ignore_string_bounds"
    );

    int2dds::common::env::set_xtypes_rti_key_compat(true);
    let enum_match = key_types_match("key_enum_2", "key_enum_1");
    let string_match = key_types_match("key_string20", "key_string10");
    int2dds::common::env::set_xtypes_rti_key_compat(false);
    assert!(enum_match, "RTI mode skips the key enum literal rule");
    assert!(string_match, "RTI mode lets ignore_string_bounds cover key strings");
}

#[test]
fn same_type_name_incompatible_types_raise_inconsistent_topic() {
    let writer_support = support(
        r#"<types><struct name="Sample" extensibility="final">
             <member name="v" type="int32"/>
           </struct></types>"#,
        "Sample",
    );
    let reader_support = support(
        r#"<types><struct name="Sample" extensibility="final">
             <member name="v" type="string"/>
           </struct></types>"#,
        "Sample",
    );
    let (pub_side, writer, sub_side, reader) = writer_reader_pair(&writer_support, &reader_support);

    assert!(pub_side.wait_inconsistent(StdDuration::from_secs(10)));
    assert!(sub_side.wait_inconsistent(StdDuration::from_secs(10)));
    assert!(wait_for_writer_status(
        &writer,
        StatusMask::PUBLICATION_MATCHED,
        Duration::from_seconds(2)
    )
    .is_err());
    assert!(wait_for_reader_status(
        &reader,
        StatusMask::SUBSCRIPTION_MATCHED,
        Duration::from_seconds(1)
    )
    .is_err());
    assert!(pub_side.listener.calls.load(Ordering::SeqCst) >= 1);
    assert!(sub_side.listener.calls.load(Ordering::SeqCst) >= 1);

    pub_side.delete();
    sub_side.delete();
}
