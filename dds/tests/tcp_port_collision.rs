//! Per-participant TCP listen ports.
//!
//! Two participants in one process that pin the *same* TCP listen port must not
//! panic — the second creation has to surface a recoverable error so the
//! application can react (a `panic!` would also be UB across the C ABI). Pinning
//! *distinct* ports lets both coexist, which is the whole point of moving the
//! TCP listen port into per-participant QoS.

mod common;

use common::*;
use int2dds::{
    domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
    infrastructure::{qos_policy::PropertyQosPolicy, status::StatusMask},
};

/// Pure-TCP participant QoS pinned to `bind_port`. `initial_peers` is required
/// by TCP (no multicast for discovery); it only needs to be non-empty and points
/// at a port nobody answers, so no real dial happens during the test.
fn tcp_qos(bind_port: u16) -> DomainParticipantQos {
    let mut property = PropertyQosPolicy::default();
    property.add_property("int2dds.transport", "tcp", false);
    property.add_property("int2dds.initial_peers", "127.0.0.1:1", false);
    property.set_tcp_bind_port(bind_port);
    DomainParticipantQos { property, ..Default::default() }
}

#[test]
fn second_tcp_participant_on_busy_port_errors_without_panicking() {
    let factory = DomainParticipantFactory::get_instance();
    let domain = next_domain_id();
    const PORT: u16 = 17400;

    // First participant binds the pinned port and must stay alive while the
    // second one attempts the same port.
    let first = factory
        .create_participant(domain, tcp_qos(PORT), None, StatusMask::default())
        .expect("first TCP participant should bind the pinned listen port");

    let second = factory.create_participant(domain, tcp_qos(PORT), None, StatusMask::default());
    assert!(
        second.is_err(),
        "second TCP participant must fail on the in-use listen port instead of binding it"
    );

    drop(first);
}

#[test]
fn distinct_ports_let_two_tcp_participants_coexist() {
    let factory = DomainParticipantFactory::get_instance();
    let domain = next_domain_id();

    // Same process, same domain, distinct per-participant TCP listen ports.
    let a = factory
        .create_participant(domain, tcp_qos(17410), None, StatusMask::default())
        .expect("participant A binds 17410");
    let b = factory
        .create_participant(domain, tcp_qos(17412), None, StatusMask::default())
        .expect("participant B binds 17412 (distinct port, no collision)");

    drop(a);
    drop(b);
}
