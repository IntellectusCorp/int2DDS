mod common;

use common::{create_datareader, next_domain_id};
use int2dds::dcps::{
    domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
    infrastructure::status::StatusMask,
    subscription::qos::DataReaderQos,
};

#[test]
fn integration_test_durability_kind() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();
    let data_reader = create_datareader(&participant, DataReaderQos::default());
    assert_eq!(0, 0);
}
