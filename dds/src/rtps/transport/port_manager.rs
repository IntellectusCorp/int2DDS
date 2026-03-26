//! Port number calculation for RTPS communication.
//!
//! This module implements the PortManager which calculates port numbers for
//! RTPS communication based on domain ID and participant ID following the RTPS
//! specification (Figure 9.6.2.3).
//!
//! Also provides TCP physical port calculation and logical port classification
//! for single-port multiplexed TCP mode.

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

    /// Get the TCP physical port (base port for a domain)
    /// = PB + DG * domain_id
    pub(crate) fn get_tcp_physical_port(domain_id: u32) -> u16 {
        (Self::PB_DEFAULT_BASE_NUMBER + Self::DG_DOMAIN_ID_GAIN * domain_id) as u16
    }

    /// Check if a logical port is a discovery unicast port for the given domain.
    /// Discovery ports: PB + DG*domain + D1 + PG*participant (D1=10, PG=2)
    /// → offsets from base are 10, 12, 14, ... (even offsets >= D1)
    pub(crate) fn is_discovery_unicast_port(domain_id: u32, logical_port: u16) -> bool {
        let base = Self::get_tcp_physical_port(domain_id) as u32;
        let port = logical_port as u32;
        if port < base + Self::D1_ADDITIONAL_OFFSET {
            return false;
        }
        (port - base - Self::D1_ADDITIONAL_OFFSET) % Self::PG_PARTICIPANT_ID_GAIN == 0
    }

    /// Check if a logical port is a user data unicast port for the given domain.
    /// User ports: PB + DG*domain + D3 + PG*participant (D3=11, PG=2)
    /// → offsets from base are 11, 13, 15, ... (odd offsets >= D3)
    pub(crate) fn is_user_unicast_port(domain_id: u32, logical_port: u16) -> bool {
        let base = Self::get_tcp_physical_port(domain_id) as u32;
        let port = logical_port as u32;
        if port < base + Self::D3_ADDITIONAL_OFFSET {
            return false;
        }
        (port - base - Self::D3_ADDITIONAL_OFFSET) % Self::PG_PARTICIPANT_ID_GAIN == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tcp_physical_port() {
        assert_eq!(PortManager::get_tcp_physical_port(0), 7400);
        assert_eq!(PortManager::get_tcp_physical_port(1), 7650);
        assert_eq!(PortManager::get_tcp_physical_port(2), 7900);
    }

    #[test]
    fn test_is_discovery_unicast_port() {
        // domain=0: pid=0→7410, pid=1→7412, pid=2→7414
        assert!(PortManager::is_discovery_unicast_port(0, 7410));
        assert!(PortManager::is_discovery_unicast_port(0, 7412));
        assert!(PortManager::is_discovery_unicast_port(0, 7414));

        assert!(!PortManager::is_discovery_unicast_port(0, 7400)); // base port
        assert!(!PortManager::is_discovery_unicast_port(0, 7409)); // below D1
        assert!(!PortManager::is_discovery_unicast_port(0, 7411)); // user port

        // domain=1: pid=0→7660
        assert!(PortManager::is_discovery_unicast_port(1, 7660));
        assert!(!PortManager::is_discovery_unicast_port(1, 7661));
        assert!(!PortManager::is_discovery_unicast_port(1, 7410)); // wrong domain
    }

    #[test]
    fn test_is_user_unicast_port() {
        // domain=0: pid=0→7411, pid=1→7413, pid=2→7415
        assert!(PortManager::is_user_unicast_port(0, 7411));
        assert!(PortManager::is_user_unicast_port(0, 7413));
        assert!(PortManager::is_user_unicast_port(0, 7415));

        assert!(!PortManager::is_user_unicast_port(0, 7400)); // base port
        assert!(!PortManager::is_user_unicast_port(0, 7410)); // discovery port
        assert!(!PortManager::is_user_unicast_port(0, 7409)); // below D3

        // domain=1: pid=0→7661
        assert!(PortManager::is_user_unicast_port(1, 7661));
        assert!(!PortManager::is_user_unicast_port(1, 7660));
    }

    #[test]
    fn test_consistency_with_get_functions() {
        // is_discovery_unicast_port should return true for ports generated by get_discovery_traffic_unicast_port
        for domain in 0..3 {
            for pid in 0..10 {
                let port = PortManager::get_discovery_traffic_unicast_port(domain, pid);
                assert!(
                    PortManager::is_discovery_unicast_port(domain, port),
                    "discovery port {} (domain={}, pid={}) not recognized",
                    port,
                    domain,
                    pid
                );
                assert!(
                    !PortManager::is_user_unicast_port(domain, port),
                    "discovery port {} incorrectly classified as user port",
                    port
                );

                let port = PortManager::get_user_traffic_unicast_port(domain, pid);
                assert!(
                    PortManager::is_user_unicast_port(domain, port),
                    "user port {} (domain={}, pid={}) not recognized",
                    port,
                    domain,
                    pid
                );
                assert!(
                    !PortManager::is_discovery_unicast_port(domain, port),
                    "user port {} incorrectly classified as discovery port",
                    port
                );
            }
        }
    }
}
