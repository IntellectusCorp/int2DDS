//! Port number calculation for RTPS communication.
//!
//! This module implements the PortManager which calculates port numbers for
//! RTPS communication based on domain ID and participant ID following the RTPS
//! specification (Figure 9.6.2.3).
//!
//! Also provides the TCP listening port, which follows the same shape so that
//! several participants can share a host without leaving their domain block.

use std::io;

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

    pub(crate) fn calculate_discovery_traffic_multicast_port(domain_id: u32) -> io::Result<u16> {
        Self::calculate_port(domain_id, Self::D0_ADDITIONAL_OFFSET, None)
    }

    pub(crate) fn calculate_discovery_traffic_unicast_port(
        domain_id: u32,
        participant_id: u32,
    ) -> io::Result<u16> {
        // Env override pins the base port
        if let Some(base) = crate::common::env::get_meta_port_override() {
            // Use 2 * participant_id as the offset
            return Ok(base.saturating_add(2u16.saturating_mul(participant_id as u16)));
        }
        Self::calculate_port(domain_id, Self::D1_ADDITIONAL_OFFSET, Some(participant_id))
    }

    pub(crate) fn calculate_user_traffic_multicast_port(domain_id: u32) -> io::Result<u16> {
        Self::calculate_port(domain_id, Self::D2_ADDITIONAL_OFFSET, None)
    }

    pub(crate) fn calculate_user_traffic_unicast_port(
        domain_id: u32,
        participant_id: u32,
    ) -> io::Result<u16> {
        // Env override pins the base port
        if let Some(base) = crate::common::env::get_user_port_override() {
            // Use 2 * participant_id as the offset
            return Ok(base.saturating_add(2u16.saturating_mul(participant_id as u16)));
        }
        Self::calculate_port(domain_id, Self::D3_ADDITIONAL_OFFSET, Some(participant_id))
    }

    /// Highest participant id whose TCP port still falls inside its own domain
    /// block: one step further the offset equals DG, which is the next domain's
    /// base port.
    pub(crate) const MAX_TCP_PARTICIPANT_ID: u32 =
        Self::DG_DOMAIN_ID_GAIN / Self::PG_PARTICIPANT_ID_GAIN - 1;

    /// Calculate the TCP physical port for one participant
    /// = PB + DG * domain_id + PG * participant_id
    pub(crate) fn calculate_tcp_physical_port(
        domain_id: u32,
        participant_id: u32,
    ) -> io::Result<u16> {
        Self::calculate_port(domain_id, 0, Some(participant_id))
    }

    // PB + DG * domain_id + offset (+ PG * participant_id for unicast), rejected when above 65535
    fn calculate_port(domain_id: u32, offset: u32, participant_id: Option<u32>) -> io::Result<u16> {
        let port = u64::from(Self::PB_DEFAULT_BASE_NUMBER)
            + u64::from(Self::DG_DOMAIN_ID_GAIN) * u64::from(domain_id)
            + u64::from(offset)
            + participant_id
                .map_or(0, |id| u64::from(Self::PG_PARTICIPANT_ID_GAIN) * u64::from(id));

        u16::try_from(port).map_err(|_| {
            let message = match participant_id {
                Some(id) => {
                    format!(
                        "port {} for domain {} participant {} exceeds 65535",
                        port, domain_id, id
                    )
                }
                None => format!("port {} for domain {} exceeds 65535", port, domain_id),
            };
            io::Error::new(io::ErrorKind::InvalidInput, message)
        })
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
        assert_eq!(PortManager::calculate_discovery_traffic_unicast_port(0, 0).unwrap(), 7410);
        // 7400 + 250*1 + 11 + 2*2 = 7665
        assert_eq!(PortManager::calculate_user_traffic_unicast_port(1, 2).unwrap(), 7665);
    }

    #[test]
    fn port_above_u16_range_is_rejected_instead_of_wrapped() {
        // 7400 + 250*232 + 1 = 65401
        assert_eq!(PortManager::calculate_user_traffic_multicast_port(232).unwrap(), 65401);

        // 7400 + 250*233 + 0 = 65650
        let error = PortManager::calculate_discovery_traffic_multicast_port(233).unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
        assert_eq!(error.to_string(), "port 65650 for domain 233 exceeds 65535");
    }

    #[test]
    fn domain_id_near_u32_max_is_rejected_without_overflow() {
        let error = PortManager::calculate_user_traffic_multicast_port(u32::MAX).unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    }

    #[test]
    fn meta_override_ignores_domain_id() {
        let _g = ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("INT2DDS_META_PORT", "55001") };
        assert_eq!(PortManager::calculate_discovery_traffic_unicast_port(0, 0).unwrap(), 55001);
        assert_eq!(PortManager::calculate_discovery_traffic_unicast_port(5, 0).unwrap(), 55001);
        assert_eq!(PortManager::calculate_discovery_traffic_unicast_port(0, 1).unwrap(), 55003);
        assert_eq!(PortManager::calculate_discovery_traffic_unicast_port(99, 3).unwrap(), 55007);
        unsafe { std::env::remove_var("INT2DDS_META_PORT") };
    }

    #[test]
    fn user_override_ignores_domain_id() {
        let _g = ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("INT2DDS_USER_PORT", "55000") };
        assert_eq!(PortManager::calculate_user_traffic_unicast_port(0, 0).unwrap(), 55000);
        assert_eq!(PortManager::calculate_user_traffic_unicast_port(7, 2).unwrap(), 55004);
        unsafe { std::env::remove_var("INT2DDS_USER_PORT") };
    }

    #[test]
    fn user_override_saturates_near_u16_max() {
        let _g = ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("INT2DDS_USER_PORT", "65534") };
        // 65534 + 2*2 -> would overflow, must clamp to 65535
        assert_eq!(PortManager::calculate_user_traffic_unicast_port(0, 2).unwrap(), 65535);
        unsafe { std::env::remove_var("INT2DDS_USER_PORT") };
    }

    #[test]
    fn test_tcp_physical_port() {
        assert_eq!(PortManager::calculate_tcp_physical_port(0, 0).unwrap(), 7400);
        assert_eq!(PortManager::calculate_tcp_physical_port(1, 0).unwrap(), 7650);
        assert_eq!(PortManager::calculate_tcp_physical_port(2, 0).unwrap(), 7900);
        assert_eq!(PortManager::calculate_tcp_physical_port(0, 1).unwrap(), 7402);
        assert_eq!(PortManager::calculate_tcp_physical_port(1, 3).unwrap(), 7656);
    }

    #[test]
    fn tcp_port_of_last_participant_stays_in_its_domain_block() {
        let last = PortManager::calculate_tcp_physical_port(0, PortManager::MAX_TCP_PARTICIPANT_ID)
            .unwrap();
        assert_eq!(last, 7648);
        assert!(last < PortManager::calculate_tcp_physical_port(1, 0).unwrap());
    }

    #[test]
    fn tcp_port_above_u16_range_is_rejected_instead_of_wrapped() {
        // 7400 + 250*232 + 2*68 = 65536
        let error = PortManager::calculate_tcp_physical_port(232, 68).unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
        assert_eq!(error.to_string(), "port 65536 for domain 232 participant 68 exceeds 65535");
    }
}
