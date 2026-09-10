//! Port number calculation for RTPS communication.
//!
//! This module implements the PortManager which calculates port numbers for
//! RTPS communication based on domain ID and participant ID following the RTPS
//! specification (Figure 9.6.2.3).
//!
//! Also provides the TCP listening port, which follows the same shape so that
//! several participants can share a host without leaving their domain block.

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
        // Env override pins the base port
        if let Some(base) = crate::common::env::get_meta_port_override() {
            // Use 2 * participant_id as the offset
            return base.saturating_add(2u16.saturating_mul(participant_id as u16));
        }
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
        // Env override pins the base port
        if let Some(base) = crate::common::env::get_user_port_override() {
            // Use 2 * participant_id as the offset
            return base.saturating_add(2u16.saturating_mul(participant_id as u16));
        }
        (Self::PB_DEFAULT_BASE_NUMBER
            + Self::DG_DOMAIN_ID_GAIN * domain_id
            + Self::D3_ADDITIONAL_OFFSET
            + Self::PG_PARTICIPANT_ID_GAIN * participant_id) as u16
    }

    /// Highest participant id whose TCP port still falls inside its own domain
    /// block: one step further the offset equals DG, which is the next domain's
    /// base port.
    pub(crate) const MAX_TCP_PARTICIPANT_ID: u32 =
        Self::DG_DOMAIN_ID_GAIN / Self::PG_PARTICIPANT_ID_GAIN - 1;

    /// Get the TCP physical port for one participant
    /// = PB + DG * domain_id + PG * participant_id
    pub(crate) fn get_tcp_physical_port(domain_id: u32, participant_id: u32) -> u16 {
        (Self::PB_DEFAULT_BASE_NUMBER
            + Self::DG_DOMAIN_ID_GAIN * domain_id
            + Self::PG_PARTICIPANT_ID_GAIN * participant_id) as u16
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Serialize env-mutating tests since std::env is process-global
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn standard_formula_when_env_unset() {
        let _g = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::remove_var("INT2DDS_META_PORT");
            std::env::remove_var("INT2DDS_USER_PORT");
        }
        // 7400 + 250*0 + 10 + 2*0 = 7410
        assert_eq!(PortManager::get_discovery_traffic_unicast_port(0, 0), 7410);
        // 7400 + 250*1 + 11 + 2*2 = 7665
        assert_eq!(PortManager::get_user_traffic_unicast_port(1, 2), 7665);
    }

    #[test]
    fn meta_override_ignores_domain_id() {
        let _g = ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("INT2DDS_META_PORT", "55001") };
        assert_eq!(PortManager::get_discovery_traffic_unicast_port(0, 0), 55001);
        assert_eq!(PortManager::get_discovery_traffic_unicast_port(5, 0), 55001);
        assert_eq!(PortManager::get_discovery_traffic_unicast_port(0, 1), 55003);
        assert_eq!(PortManager::get_discovery_traffic_unicast_port(99, 3), 55007);
        unsafe { std::env::remove_var("INT2DDS_META_PORT") };
    }

    #[test]
    fn user_override_ignores_domain_id() {
        let _g = ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("INT2DDS_USER_PORT", "55000") };
        assert_eq!(PortManager::get_user_traffic_unicast_port(0, 0), 55000);
        assert_eq!(PortManager::get_user_traffic_unicast_port(7, 2), 55004);
        unsafe { std::env::remove_var("INT2DDS_USER_PORT") };
    }

    #[test]
    fn user_override_saturates_near_u16_max() {
        let _g = ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("INT2DDS_USER_PORT", "65534") };
        // 65534 + 2*2 -> would overflow, must clamp to 65535
        assert_eq!(PortManager::get_user_traffic_unicast_port(0, 2), 65535);
        unsafe { std::env::remove_var("INT2DDS_USER_PORT") };
    }

    #[test]
    fn test_tcp_physical_port() {
        assert_eq!(PortManager::get_tcp_physical_port(0, 0), 7400);
        assert_eq!(PortManager::get_tcp_physical_port(1, 0), 7650);
        assert_eq!(PortManager::get_tcp_physical_port(2, 0), 7900);
        assert_eq!(PortManager::get_tcp_physical_port(0, 1), 7402);
        assert_eq!(PortManager::get_tcp_physical_port(1, 3), 7656);
    }

    #[test]
    fn tcp_port_of_last_participant_stays_in_its_domain_block() {
        let last = PortManager::get_tcp_physical_port(0, PortManager::MAX_TCP_PARTICIPANT_ID);
        assert_eq!(last, 7648);
        assert!(last < PortManager::get_tcp_physical_port(1, 0));
    }
}
