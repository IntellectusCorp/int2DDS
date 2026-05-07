//! Verifies the `INT2DDS_MULTICAST_TTL` environment-variable fallback resolved by
//! `TransportConfig::from_property` when the `PropertyQosPolicy` carries no
//! explicit `int2dds.transport.UDPv4.multicast_ttl` entry.
//!
//! Lives in its own integration-test binary so the env-variable mutations do not
//! race with other tests.

use int2dds::common::env::{get_multicast_ttl_override, set_multicast_ttl};

const ENV_KEY: &str = "INT2DDS_MULTICAST_TTL";

fn clear_env() {
    unsafe { std::env::remove_var(ENV_KEY) };
}

#[test]
fn env_override_round_trips_through_helpers() {
    clear_env();
    assert_eq!(get_multicast_ttl_override(), None, "unset → None");

    set_multicast_ttl(64);
    assert_eq!(get_multicast_ttl_override(), Some(64));

    unsafe { std::env::set_var(ENV_KEY, "abc") };
    assert_eq!(get_multicast_ttl_override(), None, "non-numeric → None");

    unsafe { std::env::set_var(ENV_KEY, "256") };
    assert_eq!(get_multicast_ttl_override(), None, "out-of-u8 range → None");

    clear_env();
}
