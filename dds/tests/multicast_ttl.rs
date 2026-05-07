//! End-to-end check that `PropertyQosPolicy` survives the JSON profile loader
//! and reaches `DomainParticipantQos.property`. Wire-level TTL on the IP header
//! is verified manually with Wireshark per `docs/property_qos_policy_plan.md`.

mod common;

use std::io::Write;
use tempfile::NamedTempFile;

use int2dds::dcps::domain::domain_participant_factory::DomainParticipantFactory;

fn write_profile_with_ttl(ttl: &str) -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    let json = format!(
        r#"{{
            "name": "MulticastTtlLib",
            "qos_profiles": [{{
                "name": "TtlProfile",
                "domain_participant_qos": {{
                    "property": {{
                        "value": [
                            {{ "name": "int2dds.transport.UDPv4.multicast_ttl", "value": "{}", "propagate": false }}
                        ]
                    }}
                }}
            }}]
        }}"#,
        ttl
    );
    file.write_all(json.as_bytes()).unwrap();
    file
}

#[test]
fn participant_qos_property_loaded_from_profile() {
    let file = write_profile_with_ttl("32");
    let factory = DomainParticipantFactory::get_instance();
    factory.load_profiles(&[file.path()]).unwrap();

    let qos = factory
        .get_participant_qos_from_profile("MulticastTtlLib::TtlProfile")
        .expect("profile resolves");

    assert_eq!(
        qos.property.find_property("int2dds.transport.UDPv4.multicast_ttl"),
        Some("32"),
        "property must be reachable on the resolved DomainParticipantQos"
    );
}
