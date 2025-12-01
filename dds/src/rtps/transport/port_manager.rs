//! Port number calculation for RTPS communication.
//!
//! This module implements the PortManager which calculates UDP port numbers for
//! RTPS communication based on domain ID and participant ID following the RTPS
//! specification (Figure 9.6.2.3).

pub(crate) struct PortManager {}

impl PortManager {
    //Figure 9.6.2.3
    const PB_DEFAULT_BASE_NUMBER: u32 = 7400;
    const DG_DOMAIN_ID_GAIN: u32 = 250;
    const PG_PARTICIPANT_ID_GAIN: u32 = 2;
    const D0_ADDITIONAL_OFFSET: u32 = 0;
    const D1_ADDITIONAL_OFFSET: u32 = 10;
    const D2_ADDITIONAL_OFFSET: u32 = 1;
    const D3_ADDITIONAL_OFFSET: u32 = 11;

    pub(crate) fn get_discovery_traffic_multicast_port(domain_id: u32) -> u16 {
        (Self::PB_DEFAULT_BASE_NUMBER
            + Self::DG_DOMAIN_ID_GAIN * domain_id
            + Self::D0_ADDITIONAL_OFFSET) as u16
    }

    pub(crate) fn get_discovery_traffic_unicast_port(domain_id: u32, participant_id: u32) -> u16 {
        (Self::PB_DEFAULT_BASE_NUMBER
            + Self::DG_DOMAIN_ID_GAIN * domain_id
            + Self::D1_ADDITIONAL_OFFSET
            + Self::PG_PARTICIPANT_ID_GAIN * participant_id) as u16
    }

    pub(crate) fn get_user_traffic_multicast_port(domain_id: u32) -> u16 {
        (Self::PB_DEFAULT_BASE_NUMBER
            + Self::DG_DOMAIN_ID_GAIN * domain_id
            + Self::D2_ADDITIONAL_OFFSET) as u16
    }

    pub(crate) fn get_user_traffic_unicast_port(domain_id: u32, participant_id: u32) -> u16 {
        (Self::PB_DEFAULT_BASE_NUMBER
            + Self::DG_DOMAIN_ID_GAIN * domain_id
            + Self::D3_ADDITIONAL_OFFSET
            + Self::PG_PARTICIPANT_ID_GAIN * participant_id) as u16
    }
}
