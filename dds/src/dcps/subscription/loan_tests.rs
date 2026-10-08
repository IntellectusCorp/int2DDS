//! Loan tests over an in-process writer and readers.

use std::time::{Duration as StdDuration, Instant};

use crate::{
    common::instance_handle::InstanceHandle,
    core::{error::DdsError, time::Duration},
    dcps::topic::type_support::DdsType,
    domain::{
        domain_participant::DomainParticipant,
        domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos,
    },
    infrastructure::{
        qos_policy::{
            HistoryQosPolicy, HistoryQosPolicyKind, ReliabilityQosPolicy, ReliabilityQosPolicyKind,
        },
        status::StatusMask,
    },
    publication::{
        data_writer::DataWriter,
        qos::{DataWriterQos, PublisherQos},
    },
    subscription::{
        data_reader::DataReader,
        qos::{DataReaderQos, SubscriberQos},
        sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
        subscriber::Subscriber,
    },
    test_utils::unique_domain_id,
    topic::qos::TopicQos,
};

#[derive(DdsType)]
pub(super) struct Shape {
    #[dds(key)]
    pub id: u32,
    pub size: u32,
}

pub(super) fn shape(id: u32, size: u32) -> Shape {
    Shape { id, size }
}

pub(super) const ANY_SAMPLE: &[SampleStateKind] = &[SampleStateKind::ANY_SAMPLE_STATE];
pub(super) const ANY_VIEW: &[ViewStateKind] = &[ViewStateKind::ANY_VIEW_STATE];
pub(super) const ANY_INSTANCE: &[InstanceStateKind] = &[InstanceStateKind::ANY_INSTANCE_STATE];

pub(super) fn keep_all() -> HistoryQosPolicy {
    HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true }
}

pub(super) fn keep_last(depth: i32) -> HistoryQosPolicy {
    HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepLast(depth), strict: false }
}

fn reliable() -> ReliabilityQosPolicy {
    ReliabilityQosPolicy {
        kind: ReliabilityQosPolicyKind::Reliable,
        max_blocking_time: Duration::from_seconds(1),
    }
}

pub(super) fn wait_until(what: &str, mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + StdDuration::from_secs(10);
    while !ready() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(StdDuration::from_millis(10));
    }
}

pub(super) struct Fixture {
    pub participant: DomainParticipant,
    pub subscriber: Subscriber,
    pub writer: DataWriter<Shape>,
    pub readers: Vec<DataReader<Shape>>,
}

impl Fixture {
    pub(super) fn new(reader_count: usize, reader_history: HistoryQosPolicy) -> Self {
        let domain_id = unique_domain_id();
        let participant = DomainParticipantFactory::get_instance()
            .create_participant(
                domain_id,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let topic = participant
            .create_topic::<Shape>(
                &format!("loan_{domain_id}"),
                "Shape",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let publisher = participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();
        let writer_qos =
            DataWriterQos { history: keep_all(), reliability: reliable(), ..Default::default() };
        let writer = publisher
            .create_datawriter::<Shape>(&topic, writer_qos, None, StatusMask::default())
            .unwrap();
        let subscriber = participant
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();
        let readers: Vec<DataReader<Shape>> = (0..reader_count)
            .map(|_| {
                let qos = DataReaderQos {
                    history: reader_history.clone(),
                    reliability: reliable(),
                    ..Default::default()
                };
                subscriber
                    .create_datareader::<Shape>(&topic, qos, None, StatusMask::default())
                    .unwrap()
            })
            .collect();
        wait_until("writer match", || {
            writer.get_publication_matched_status().unwrap().current_count == reader_count as i32
        });
        for reader in &readers {
            wait_until("reader match", || {
                reader.get_subscription_matched_status().unwrap().current_count == 1
            });
        }
        Self { participant, subscriber, writer, readers }
    }

    /// Writes `samples`, then waits until every reader caches `cached` changes.
    pub(super) fn write(&self, samples: &[Shape], cached: usize) {
        for sample in samples {
            self.writer.write(sample, InstanceHandle::NIL).unwrap();
        }
        for reader in &self.readers {
            wait_until("delivery", || reader.get_available_changes().unwrap().len() == cached);
        }
    }

    pub(super) fn finish(self) {
        let Self { participant, subscriber, writer, readers } = self;
        participant.delete_contained_entities().unwrap();
        DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
        drop((readers, writer, subscriber));
    }
}

#[test]
fn query_condition_filter_decodes_into_the_cache() {
    let f = Fixture::new(1, keep_all());
    f.write(&[shape(0, 10), shape(5, 50)], 2);
    let reader = &f.readers[0];
    let qc = reader
        .create_querycondition(ANY_SAMPLE, ANY_VIEW, ANY_INSTANCE, "id < %0", vec!["3".to_string()])
        .unwrap();

    let samples = reader.read_w_condition(10, qc.clone()).unwrap();

    assert_eq!(samples.len(), 1);
    assert_eq!(samples[0].data().unwrap().size, 10);
    for change in reader.get_available_changes().unwrap() {
        assert!(change.decoded().get().is_some(), "the filter decodes every candidate once");
    }
    f.finish();
}

use super::{data_sample::DataSample, loaned_samples::LoanedSamples};
use crate::core::error::DdsResult;

type Row =
    (InstanceHandle, SampleStateKind, ViewStateKind, InstanceStateKind, i32, bool, Option<u32>);

fn copy_rows(result: DdsResult<Vec<DataSample<Shape>>>) -> Result<Vec<Row>, String> {
    result
        .map(|samples| {
            samples
                .iter()
                .map(|s| {
                    let i = s.sample_info();
                    let size = s.data().ok().map(|d| d.size);
                    (
                        i.instance_handle,
                        i.sample_state,
                        i.view_state,
                        i.instance_state,
                        i.sample_rank,
                        i.valid_data,
                        size,
                    )
                })
                .collect()
        })
        .map_err(|e| format!("{e:?}"))
}

fn loan_rows(result: DdsResult<LoanedSamples<Shape>>) -> Result<Vec<Row>, String> {
    result
        .map(|samples| {
            samples
                .iter()
                .map(|s| {
                    let i = s.sample_info();
                    let size = s.data().ok().map(|d| d.size);
                    (
                        i.instance_handle,
                        i.sample_state,
                        i.view_state,
                        i.instance_state,
                        i.sample_rank,
                        i.valid_data,
                        size,
                    )
                })
                .collect()
        })
        .map_err(|e| format!("{e:?}"))
}

#[test]
fn repeated_read_loaned_lends_the_same_decoded_sample() {
    let f = Fixture::new(1, keep_all());
    f.write(&[shape(0, 10)], 1);
    let reader = &f.readers[0];

    let first = reader.read_loaned(10, ANY_SAMPLE, ANY_VIEW, ANY_INSTANCE).unwrap();
    let second = reader.read_loaned(10, ANY_SAMPLE, ANY_VIEW, ANY_INSTANCE).unwrap();

    assert_eq!(first[0].data().unwrap().size, 10);
    assert!(std::ptr::eq(first[0].data().unwrap(), second[0].data().unwrap()));
    drop((first, second));
    f.finish();
}

#[test]
fn loan_outlives_eviction_by_keep_last() {
    let f = Fixture::new(1, keep_last(1));
    f.write(&[shape(0, 1)], 1);
    let reader = &f.readers[0];
    let loan = reader.read_loaned(10, ANY_SAMPLE, ANY_VIEW, ANY_INSTANCE).unwrap();

    f.writer.write(&shape(0, 2), InstanceHandle::NIL).unwrap();
    wait_until("eviction", || {
        reader
            .read(1, ANY_SAMPLE, ANY_VIEW, ANY_INSTANCE)
            .map(|s| s[0].data().unwrap().size == 2)
            .unwrap_or(false)
    });

    assert_eq!(loan[0].data().unwrap().size, 1);
    drop(loan);
    f.finish();
}

#[test]
fn disposed_instance_yields_an_element_without_data() {
    let f = Fixture::new(1, keep_all());
    f.write(&[shape(0, 1)], 1);
    let reader = &f.readers[0];
    f.writer.dispose(&shape(0, 0), InstanceHandle::NIL).unwrap();
    wait_until("dispose", || {
        reader.read(10, ANY_SAMPLE, ANY_VIEW, ANY_INSTANCE).is_ok_and(|samples| {
            samples.iter().any(|s| {
                s.sample_info().instance_state
                    == InstanceStateKind::NOT_ALIVE_DISPOSED_INSTANCE_STATE
            })
        })
    });

    let loan = reader.take_loaned(10, ANY_SAMPLE, ANY_VIEW, ANY_INSTANCE).unwrap();

    let invalid: Vec<_> = loan.iter().filter(|s| !s.sample_info().valid_data).collect();
    assert!(!invalid.is_empty());
    for sample in invalid {
        assert!(matches!(sample.data(), Err(DdsError::NoData)));
        assert!(sample.serialized_data().is_none());
    }
    drop(loan);
    f.finish();
}

#[test]
fn loaned_and_copy_variants_select_the_same_samples() {
    let f = Fixture::new(2, keep_all());
    f.write(&[shape(0, 10), shape(1, 11), shape(2, 12), shape(0, 20)], 4);
    let (a, b) = (&f.readers[0], &f.readers[1]);
    let nil = InstanceHandle::NIL;
    let h0 = a.lookup_instance(&shape(0, 0)).unwrap();
    let h1 = a.lookup_instance(&shape(1, 0)).unwrap();
    let not_read = &[SampleStateKind::NOT_READ_SAMPLE_STATE];
    let rc_a = a.create_readcondition(not_read, ANY_VIEW, ANY_INSTANCE).unwrap();
    let rc_b = b.create_readcondition(not_read, ANY_VIEW, ANY_INSTANCE).unwrap();
    let qc = |r: &DataReader<Shape>| {
        r.create_querycondition(
            ANY_SAMPLE,
            ANY_VIEW,
            ANY_INSTANCE,
            "id < %0",
            vec!["2".to_string()],
        )
        .unwrap()
    };
    let (qc_a, qc_b) = (qc(a), qc(b));

    assert_eq!(
        copy_rows(a.read_instance(10, h0, ANY_SAMPLE, ANY_VIEW, ANY_INSTANCE)),
        loan_rows(b.read_instance_loaned(10, h0, ANY_SAMPLE, ANY_VIEW, ANY_INSTANCE)),
    );
    assert_eq!(
        copy_rows(a.read_next_instance(10, nil, ANY_SAMPLE, ANY_VIEW, ANY_INSTANCE)),
        loan_rows(b.read_next_instance_loaned(10, nil, ANY_SAMPLE, ANY_VIEW, ANY_INSTANCE)),
    );
    assert_eq!(
        copy_rows(a.read_w_condition(10, rc_a.clone())),
        loan_rows(b.read_w_condition_loaned(10, rc_b.clone())),
    );
    assert_eq!(
        copy_rows(a.read_next_instance_w_condition(10, nil, qc_a.clone())),
        loan_rows(b.read_next_instance_w_condition_loaned(10, nil, qc_b.clone())),
    );
    assert_eq!(
        copy_rows(a.read_w_condition(10, qc_a.clone())),
        loan_rows(b.read_w_condition_loaned(10, qc_b.clone())),
    );
    assert_eq!(
        copy_rows(a.read(10, ANY_SAMPLE, ANY_VIEW, ANY_INSTANCE)),
        loan_rows(b.read_loaned(10, ANY_SAMPLE, ANY_VIEW, ANY_INSTANCE)),
    );
    assert_eq!(
        copy_rows(a.take_instance(10, h1, ANY_SAMPLE, ANY_VIEW, ANY_INSTANCE)),
        loan_rows(b.take_instance_loaned(10, h1, ANY_SAMPLE, ANY_VIEW, ANY_INSTANCE)),
    );
    assert_eq!(
        copy_rows(a.take_next_instance(10, nil, ANY_SAMPLE, ANY_VIEW, ANY_INSTANCE)),
        loan_rows(b.take_next_instance_loaned(10, nil, ANY_SAMPLE, ANY_VIEW, ANY_INSTANCE)),
    );
    assert_eq!(
        copy_rows(a.take_next_instance_w_condition(10, nil, qc_a.clone())),
        loan_rows(b.take_next_instance_w_condition_loaned(10, nil, qc_b.clone())),
    );
    assert_eq!(
        copy_rows(a.take_w_condition(10, rc_a.clone())),
        loan_rows(b.take_w_condition_loaned(10, rc_b.clone())),
    );
    assert_eq!(
        copy_rows(a.take(10, ANY_SAMPLE, ANY_VIEW, ANY_INSTANCE)),
        loan_rows(b.take_loaned(10, ANY_SAMPLE, ANY_VIEW, ANY_INSTANCE)),
    );
    assert_eq!(
        copy_rows(a.take(10, ANY_SAMPLE, ANY_VIEW, ANY_INSTANCE)),
        loan_rows(b.take_loaned(10, ANY_SAMPLE, ANY_VIEW, ANY_INSTANCE)),
    );
    f.finish();
}

#[test]
fn return_loan_rejects_a_loan_from_another_reader() {
    let f = Fixture::new(2, keep_all());
    f.write(&[shape(0, 1)], 1);
    let loan = f.readers[0].read_loaned(10, ANY_SAMPLE, ANY_VIEW, ANY_INSTANCE).unwrap();

    let (err, loan) = f.readers[1].return_loan(loan).unwrap_err();

    assert!(matches!(err, DdsError::PreconditionNotMet));
    assert_eq!(f.readers[0].outstanding_loan_count(), 1);
    assert_eq!(loan[0].data().unwrap().size, 1);
    f.readers[0].return_loan(loan).unwrap();
    assert_eq!(f.readers[0].outstanding_loan_count(), 0);
    f.finish();
}

#[test]
fn delete_datareader_waits_for_return_loan() {
    let f = Fixture::new(1, keep_all());
    f.write(&[shape(0, 1)], 1);
    let reader = f.readers[0].clone();
    let loan = reader.read_loaned(10, ANY_SAMPLE, ANY_VIEW, ANY_INSTANCE).unwrap();

    assert!(matches!(
        f.subscriber.delete_datareader(reader.clone()),
        Err(DdsError::PreconditionNotMet)
    ));
    reader.return_loan(loan).unwrap();
    f.subscriber.delete_datareader(reader).unwrap();
    f.finish();
}

#[test]
fn delete_contained_entities_deletes_nothing_while_any_reader_has_a_loan() {
    let f = Fixture::new(2, keep_all());
    f.write(&[shape(0, 1)], 1);
    let loan = f.readers[1].read_loaned(10, ANY_SAMPLE, ANY_VIEW, ANY_INSTANCE).unwrap();

    assert!(matches!(f.subscriber.delete_contained_entities(), Err(DdsError::PreconditionNotMet)));
    assert!(f.readers[0].read(10, ANY_SAMPLE, ANY_VIEW, ANY_INSTANCE).is_ok());

    drop(loan);
    f.finish();
}

#[test]
fn participant_delete_contained_entities_reports_an_outstanding_loan() {
    let f = Fixture::new(1, keep_all());
    f.write(&[shape(0, 1)], 1);
    let loan = f.readers[0].read_loaned(10, ANY_SAMPLE, ANY_VIEW, ANY_INSTANCE).unwrap();

    assert!(matches!(f.participant.delete_contained_entities(), Err(DdsError::PreconditionNotMet)));

    drop(loan);
    f.finish();
}

#[test]
fn a_failed_loaned_read_leaves_no_loan() {
    let f = Fixture::new(1, keep_all());
    let reader = f.readers[0].clone();

    assert!(matches!(
        reader.read_loaned(10, ANY_SAMPLE, ANY_VIEW, ANY_INSTANCE),
        Err(DdsError::NoData)
    ));
    assert!(matches!(
        reader.take_loaned(0, ANY_SAMPLE, ANY_VIEW, ANY_INSTANCE),
        Err(DdsError::BadParameter)
    ));
    assert_eq!(reader.outstanding_loan_count(), 0);
    f.subscriber.delete_datareader(reader).unwrap();
    f.finish();
}
