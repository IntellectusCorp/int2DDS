//! Port number calculation for RTPS communication.
//!
//! This module implements the PortManager which calculates port numbers for
//! RTPS communication based on domain ID and participant ID following the RTPS
//! specification (Figure 9.6.2.3).
//!
//! Also provides the TCP listening port, which follows the same shape so that
//! several participants can share a host without leaving their domain block.

use std::io;

// Port number parameters PB, DG, PG, d0..d3 of one participant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PortManager {
    pub(crate) port_base: u32,
    pub(crate) domain_id_gain: u32,
    pub(crate) participant_id_gain: u32,
    pub(crate) discovery_multicast_offset: u32,
    pub(crate) discovery_unicast_offset: u32,
    pub(crate) user_multicast_offset: u32,
    pub(crate) user_unicast_offset: u32,
}

impl Default for PortManager {
    fn default() -> Self {
        Self {
            port_base: Self::PB_DEFAULT_BASE_NUMBER,
            domain_id_gain: Self::DG_DOMAIN_ID_GAIN,
            participant_id_gain: Self::PG_PARTICIPANT_ID_GAIN,
            discovery_multicast_offset: Self::D0_ADDITIONAL_OFFSET,
            discovery_unicast_offset: Self::D1_ADDITIONAL_OFFSET,
            user_multicast_offset: Self::D2_ADDITIONAL_OFFSET,
            user_unicast_offset: Self::D3_ADDITIONAL_OFFSET,
        }
    }
}

impl PortManager {
    //Figure 9.6.2.3
    pub(crate) const PB_DEFAULT_BASE_NUMBER: u32 = 7400;
    pub(crate) const DG_DOMAIN_ID_GAIN: u32 = 250;
    pub(crate) const PG_PARTICIPANT_ID_GAIN: u32 = 2;
    pub(crate) const D0_ADDITIONAL_OFFSET: u32 = 0;
    pub(crate) const D1_ADDITIONAL_OFFSET: u32 = 10;
    pub(crate) const D2_ADDITIONAL_OFFSET: u32 = 1;
    pub(crate) const D3_ADDITIONAL_OFFSET: u32 = 11;

    pub(crate) fn calculate_discovery_traffic_multicast_port(
        &self,
        domain_id: u32,
    ) -> io::Result<u16> {
        self.calculate_port(domain_id, self.discovery_multicast_offset, None)
    }

    pub(crate) fn calculate_discovery_traffic_unicast_port(
        &self,
        domain_id: u32,
        participant_id: u32,
    ) -> io::Result<u16> {
        self.calculate_port(domain_id, self.discovery_unicast_offset, Some(participant_id))
    }

    pub(crate) fn calculate_user_traffic_multicast_port(&self, domain_id: u32) -> io::Result<u16> {
        self.calculate_port(domain_id, self.user_multicast_offset, None)
    }

    pub(crate) fn calculate_user_traffic_unicast_port(
        &self,
        domain_id: u32,
        participant_id: u32,
    ) -> io::Result<u16> {
        self.calculate_port(domain_id, self.user_unicast_offset, Some(participant_id))
    }

    // Highest participant id whose discovery and user unicast ports both stay below
    // the next domain's base port.
    pub(crate) fn max_unicast_participant_id(&self) -> io::Result<u32> {
        let last_offset = self.discovery_unicast_offset.max(self.user_unicast_offset);

        // DG - 1 - max(d1, d3): the room left for PG * participant_id, None when negative
        let Some(last_slot_offset) =
            self.domain_id_gain.checked_sub(1).and_then(|last| last.checked_sub(last_offset))
        else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "unicast offset {} does not fit below domain id gain {}",
                    last_offset, self.domain_id_gain
                ),
            ));
        };

        Ok(last_slot_offset / self.participant_id_gain.max(1))
    }

    /// Highest participant id whose TCP port still falls inside its own domain
    /// block: one step further the offset reaches DG, which is the next domain's
    /// base port.
    pub(crate) fn max_tcp_participant_id(&self) -> u32 {
        self.domain_id_gain.saturating_sub(1) / self.participant_id_gain.max(1)
    }

    /// Calculate the TCP physical port for one participant
    /// = PB + DG * domain_id + PG * participant_id
    pub(crate) fn calculate_tcp_physical_port(
        &self,
        domain_id: u32,
        participant_id: u32,
    ) -> io::Result<u16> {
        self.calculate_port(domain_id, 0, Some(participant_id))
    }

    // PB + DG * domain_id + offset (+ PG * participant_id for unicast), rejected when 0 or above 65535
    fn calculate_port(
        &self,
        domain_id: u32,
        offset: u32,
        participant_id: Option<u32>,
    ) -> io::Result<u16> {
        let port = u64::from(self.port_base)
            + u64::from(self.domain_id_gain) * u64::from(domain_id)
            + u64::from(offset)
            + participant_id.map_or(0, |id| u64::from(self.participant_id_gain) * u64::from(id));

        let port = u16::try_from(port).map_err(|_| {
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
        })?;

        // Binding port 0 would take an ephemeral port instead of the computed one
        if port == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("computed port 0 for domain {} is not a bindable port", domain_id),
            ));
        }

        Ok(port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_formula_with_default_parameters() {
        let ports = PortManager::default();

        // 7400 + 250*0 + 10 + 2*0 = 7410
        assert_eq!(ports.calculate_discovery_traffic_unicast_port(0, 0).unwrap(), 7410);
        // 7400 + 250*1 + 11 + 2*2 = 7665
        assert_eq!(ports.calculate_user_traffic_unicast_port(1, 2).unwrap(), 7665);
    }

    #[test]
    fn custom_parameters_replace_every_term_of_the_formula() {
        let ports = PortManager {
            port_base: 10000,
            domain_id_gain: 100,
            participant_id_gain: 4,
            discovery_multicast_offset: 1,
            discovery_unicast_offset: 2,
            user_multicast_offset: 3,
            user_unicast_offset: 5,
        };

        // 10000 + 100*3 + 1 = 10301
        assert_eq!(ports.calculate_discovery_traffic_multicast_port(3).unwrap(), 10301);
        // 10000 + 100*3 + 2 + 4*2 = 10310
        assert_eq!(ports.calculate_discovery_traffic_unicast_port(3, 2).unwrap(), 10310);
        // 10000 + 100*3 + 3 = 10303
        assert_eq!(ports.calculate_user_traffic_multicast_port(3).unwrap(), 10303);
        // 10000 + 100*3 + 5 + 4*2 = 10313
        assert_eq!(ports.calculate_user_traffic_unicast_port(3, 2).unwrap(), 10313);
        // 10000 + 100*3 + 4*2 = 10308
        assert_eq!(ports.calculate_tcp_physical_port(3, 2).unwrap(), 10308);
        // 100 / 4 - 1 = 24
        assert_eq!(ports.max_tcp_participant_id(), 24);
        // (100 - 1 - max(2, 5)) / 4 = 23
        assert_eq!(ports.max_unicast_participant_id().unwrap(), 23);
    }

    #[test]
    fn last_unicast_participant_id_stays_below_the_next_domain() {
        let ports = PortManager::default();

        // (250 - 1 - 11) / 2 = 119
        assert_eq!(ports.max_unicast_participant_id().unwrap(), 119);

        // Domain 1 starts at 7400 + 250 = 7650
        let next_domain_base_port = 7650;

        // 7400 + 11 + 2*119 = 7649
        assert!(ports.calculate_user_traffic_unicast_port(0, 119).unwrap() < next_domain_base_port);
        // 7400 + 11 + 2*120 = 7651
        assert!(
            ports.calculate_user_traffic_unicast_port(0, 120).unwrap() >= next_domain_base_port
        );
    }

    #[test]
    fn port_above_u16_range_is_rejected_instead_of_wrapped() {
        let ports = PortManager::default();

        // 7400 + 250*232 + 1 = 65401
        assert_eq!(ports.calculate_user_traffic_multicast_port(232).unwrap(), 65401);

        // 7400 + 250*233 + 0 = 65650
        let error = ports.calculate_discovery_traffic_multicast_port(233).unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
        assert_eq!(error.to_string(), "port 65650 for domain 233 exceeds 65535");
    }

    #[test]
    fn domain_id_near_u32_max_is_rejected_without_overflow() {
        let error =
            PortManager::default().calculate_user_traffic_multicast_port(u32::MAX).unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    }

    #[test]
    fn test_tcp_physical_port() {
        let ports = PortManager::default();

        assert_eq!(ports.calculate_tcp_physical_port(0, 0).unwrap(), 7400);
        assert_eq!(ports.calculate_tcp_physical_port(1, 0).unwrap(), 7650);
        assert_eq!(ports.calculate_tcp_physical_port(2, 0).unwrap(), 7900);
        assert_eq!(ports.calculate_tcp_physical_port(0, 1).unwrap(), 7402);
        assert_eq!(ports.calculate_tcp_physical_port(1, 3).unwrap(), 7656);
    }

    #[test]
    fn tcp_port_of_last_participant_stays_in_its_domain_block() {
        let ports = PortManager::default();

        let last = ports.calculate_tcp_physical_port(0, ports.max_tcp_participant_id()).unwrap();
        assert_eq!(last, 7648);
        assert!(last < ports.calculate_tcp_physical_port(1, 0).unwrap());
    }

    #[test]
    fn tcp_port_above_u16_range_is_rejected_instead_of_wrapped() {
        // 7400 + 250*232 + 2*68 = 65536
        let error = PortManager::default().calculate_tcp_physical_port(232, 68).unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
        assert_eq!(error.to_string(), "port 65536 for domain 232 participant 68 exceeds 65535");
    }

    #[test]
    fn port_zero_is_rejected_instead_of_bound_as_ephemeral() {
        let ports = PortManager { port_base: 0, ..PortManager::default() };

        // 0 + 250*0 + 2*0 = 0
        let error = ports.calculate_tcp_physical_port(0, 0).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);

        // 0 + 250*0 + 0 = 0
        let error = ports.calculate_discovery_traffic_multicast_port(0).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    }

    #[test]
    fn last_tcp_participant_id_counts_a_partial_last_slot() {
        let ports = PortManager { domain_id_gain: 21, ..PortManager::default() };

        // (21 - 1) / 2 = 10
        assert_eq!(ports.max_tcp_participant_id(), 10);

        // 7400 + 2*10 = 7420 stays below the next domain base 7400 + 21 = 7421
        let last_port = ports.calculate_tcp_physical_port(0, 10).unwrap();
        assert!(last_port < ports.calculate_tcp_physical_port(1, 0).unwrap());
    }
}
