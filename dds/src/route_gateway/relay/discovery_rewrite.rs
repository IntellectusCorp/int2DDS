//! In-place rewrite of a discovery announcement.
//!
//! A relayed participant must look, to the receiving network, like a plain
//! local participant that happens to live at the gateway's address. Only the
//! locators and the domain id have to change, and both are fixed width fields,
//! so the payload is edited in place: no parameter is inserted or removed and
//! every RTPS length field upstream stays valid. Locators that must disappear
//! are turned into PID_PAD, which every parameter list reader skips.
//!
//! An endpoint announcement needs the same treatment for the opposite reason.
//! It carries the addresses its own network gave it, which mean nothing on the
//! receiving one, so they are removed and the endpoint is left to inherit its
//! participant's locators, which the gateway has already rewritten.

use std::{
    net::{Ipv4Addr, SocketAddrV4},
    time::Duration,
};

const ENCAPSULATION_HEADER_LEN: usize = 4;
const PARAMETER_HEADER_LEN: usize = 4;
const LOCATOR_VALUE_LEN: usize = 24;
const DURATION_VALUE_LEN: usize = 8;

const PID_PAD: u16 = 0x0000;
const PID_SENTINEL: u16 = 0x0001;
const PID_PARTICIPANT_LEASE_DURATION: u16 = 0x0002;
const PID_DOMAIN_ID: u16 = 0x000F;
const PID_UNICAST_LOCATOR: u16 = 0x002F;
const PID_MULTICAST_LOCATOR: u16 = 0x0030;
const PID_DEFAULT_UNICAST_LOCATOR: u16 = 0x0031;
const PID_METATRAFFIC_UNICAST_LOCATOR: u16 = 0x0032;
const PID_METATRAFFIC_MULTICAST_LOCATOR: u16 = 0x0033;
const PID_DEFAULT_MULTICAST_LOCATOR: u16 = 0x0048;

const LOCATOR_KIND_UDP_V4: i32 = 1;

/// The two addresses a participant is reachable at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SpdpEndpoints {
    pub(crate) metatraffic: SocketAddrV4,
    pub(crate) user_data: SocketAddrV4,
}

struct Parameter {
    id: u16,
    /// Offset of the parameter id field inside the payload.
    header_offset: usize,
    value_offset: usize,
    value_len: usize,
}

/// Reads the first metatraffic and default unicast locator out of an
/// announcement. Both are required: a participant that advertises neither
/// cannot be reached and is not worth a table entry.
pub(crate) fn read_endpoints(payload: &[u8]) -> Option<SpdpEndpoints> {
    let little_endian = payload_is_little_endian(payload)?;
    let parameters = parse(payload, little_endian);

    let mut metatraffic = None;
    let mut user_data = None;
    for parameter in &parameters {
        let target = match parameter.id {
            PID_METATRAFFIC_UNICAST_LOCATOR => &mut metatraffic,
            PID_DEFAULT_UNICAST_LOCATOR => &mut user_data,
            _ => continue,
        };
        if target.is_none() {
            *target = read_locator(payload, parameter, little_endian);
        }
    }

    Some(SpdpEndpoints { metatraffic: metatraffic?, user_data: user_data? })
}

/// Every unicast address the announcement advertises. A participant with
/// several interfaces advertises one locator per interface, and all of them
/// name the same machine, so a policy weighing the participant has to see the
/// whole set rather than whichever one happens to come first.
pub(crate) fn read_unicast_addresses(payload: &[u8]) -> Vec<Ipv4Addr> {
    let Some(little_endian) = payload_is_little_endian(payload) else {
        return Vec::new();
    };

    let mut addresses = Vec::new();
    for parameter in parse(payload, little_endian) {
        if !matches!(
            parameter.id,
            PID_METATRAFFIC_UNICAST_LOCATOR | PID_DEFAULT_UNICAST_LOCATOR | PID_UNICAST_LOCATOR
        ) {
            continue;
        }
        if let Some(locator) = read_locator(payload, &parameter, little_endian) {
            let address = *locator.ip();
            if !addresses.contains(&address) {
                addresses.push(address);
            }
        }
    }
    addresses
}

/// How long the participant asks to be remembered for. Absent when the
/// announcement leaves the lease at its default. Only whole seconds are read,
/// because a table swept once a second cannot act on anything finer.
pub(crate) fn read_lease(payload: &[u8]) -> Option<Duration> {
    let little_endian = payload_is_little_endian(payload)?;
    let parameters = parse(payload, little_endian);

    let lease = parameters.iter().find(|p| p.id == PID_PARTICIPANT_LEASE_DURATION)?;
    if lease.value_len < DURATION_VALUE_LEN {
        return None;
    }
    let seconds = read_u32(payload, lease.value_offset, little_endian) as i32;
    if seconds <= 0 {
        return None;
    }
    Some(Duration::from_secs(seconds as u64))
}

/// Replaces every advertised address with the gateway's own and retargets the
/// announcement at the local domain. Returns false when the payload is not a
/// parameter list this function can walk.
pub(crate) fn rewrite(
    payload: &mut [u8],
    gateway_ip: Ipv4Addr,
    metatraffic_port: u16,
    user_data_port: u16,
    domain_id: u32,
) -> bool {
    let Some(little_endian) = payload_is_little_endian(payload) else {
        return false;
    };
    let parameters = parse(payload, little_endian);

    let mut metatraffic_written = false;
    let mut user_data_written = false;
    for parameter in &parameters {
        match parameter.id {
            PID_DOMAIN_ID => {
                write_u32(payload, parameter.value_offset, domain_id, little_endian);
            }
            PID_METATRAFFIC_UNICAST_LOCATOR => {
                if metatraffic_written {
                    neutralize(payload, parameter, little_endian);
                } else {
                    metatraffic_written = write_locator(
                        payload,
                        parameter,
                        gateway_ip,
                        metatraffic_port,
                        little_endian,
                    );
                }
            }
            PID_DEFAULT_UNICAST_LOCATOR => {
                if user_data_written {
                    neutralize(payload, parameter, little_endian);
                } else {
                    user_data_written = write_locator(
                        payload,
                        parameter,
                        gateway_ip,
                        user_data_port,
                        little_endian,
                    );
                }
            }
            // A relayed participant is only reachable through the gateway, so
            // it must not claim to be on any multicast group of this network.
            PID_METATRAFFIC_MULTICAST_LOCATOR | PID_DEFAULT_MULTICAST_LOCATOR => {
                neutralize(payload, parameter, little_endian);
            }
            _ => {}
        }
    }

    metatraffic_written && user_data_written
}

/// Takes the addresses out of an endpoint announcement. A receiver left with
/// none falls back to the announcing participant's locators, which is exactly
/// the gateway.
pub(crate) fn strip_endpoint_locators(payload: &mut [u8]) {
    let Some(little_endian) = payload_is_little_endian(payload) else {
        return;
    };

    for parameter in &parse(payload, little_endian) {
        if matches!(parameter.id, PID_UNICAST_LOCATOR | PID_MULTICAST_LOCATOR) {
            neutralize(payload, parameter, little_endian);
        }
    }
}

fn parse(payload: &[u8], little_endian: bool) -> Vec<Parameter> {
    let mut parameters = Vec::new();
    let mut pos = ENCAPSULATION_HEADER_LEN;

    while pos + PARAMETER_HEADER_LEN <= payload.len() {
        let id = read_u16(payload, pos, little_endian);
        if id == PID_SENTINEL {
            break;
        }
        let value_len = read_u16(payload, pos + 2, little_endian) as usize;
        let value_offset = pos + PARAMETER_HEADER_LEN;
        if value_offset + value_len > payload.len() {
            break;
        }
        parameters.push(Parameter { id, header_offset: pos, value_offset, value_len });
        pos = value_offset + value_len;
    }

    parameters
}

fn read_locator(
    payload: &[u8],
    parameter: &Parameter,
    little_endian: bool,
) -> Option<SocketAddrV4> {
    if parameter.value_len < LOCATOR_VALUE_LEN {
        return None;
    }
    let port = read_u32(payload, parameter.value_offset + 4, little_endian);
    let address = parameter.value_offset + 8;
    let ip = Ipv4Addr::new(
        payload[address + 12],
        payload[address + 13],
        payload[address + 14],
        payload[address + 15],
    );
    Some(SocketAddrV4::new(ip, port as u16))
}

fn write_locator(
    payload: &mut [u8],
    parameter: &Parameter,
    ip: Ipv4Addr,
    port: u16,
    little_endian: bool,
) -> bool {
    if parameter.value_len < LOCATOR_VALUE_LEN {
        return false;
    }
    write_u32(payload, parameter.value_offset, LOCATOR_KIND_UDP_V4 as u32, little_endian);
    write_u32(payload, parameter.value_offset + 4, port as u32, little_endian);
    let address = parameter.value_offset + 8;
    payload[address..address + 16].fill(0);
    payload[address + 12..address + 16].copy_from_slice(&ip.octets());
    true
}

fn neutralize(payload: &mut [u8], parameter: &Parameter, little_endian: bool) {
    write_u16(payload, parameter.header_offset, PID_PAD, little_endian);
}

fn payload_is_little_endian(payload: &[u8]) -> Option<bool> {
    if payload.len() < ENCAPSULATION_HEADER_LEN {
        return None;
    }
    Some(payload[1] & 0x01 != 0)
}

fn read_u16(buf: &[u8], offset: usize, little_endian: bool) -> u16 {
    let raw = [buf[offset], buf[offset + 1]];
    if little_endian {
        u16::from_le_bytes(raw)
    } else {
        u16::from_be_bytes(raw)
    }
}

fn read_u32(buf: &[u8], offset: usize, little_endian: bool) -> u32 {
    let raw = [buf[offset], buf[offset + 1], buf[offset + 2], buf[offset + 3]];
    if little_endian {
        u32::from_le_bytes(raw)
    } else {
        u32::from_be_bytes(raw)
    }
}

fn write_u16(buf: &mut [u8], offset: usize, value: u16, little_endian: bool) {
    let raw = if little_endian { value.to_le_bytes() } else { value.to_be_bytes() };
    buf[offset..offset + 2].copy_from_slice(&raw);
}

fn write_u32(buf: &mut [u8], offset: usize, value: u32, little_endian: bool) {
    let raw = if little_endian { value.to_le_bytes() } else { value.to_be_bytes() };
    buf[offset..offset + 4].copy_from_slice(&raw);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn locator_value(ip: Ipv4Addr, port: u16) -> Vec<u8> {
        let mut value = Vec::with_capacity(LOCATOR_VALUE_LEN);
        value.extend_from_slice(&LOCATOR_KIND_UDP_V4.to_le_bytes());
        value.extend_from_slice(&(port as u32).to_le_bytes());
        value.extend_from_slice(&[0u8; 12]);
        value.extend_from_slice(&ip.octets());
        value
    }

    fn parameter(id: u16, value: &[u8]) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&id.to_le_bytes());
        buf.extend_from_slice(&(value.len() as u16).to_le_bytes());
        buf.extend_from_slice(value);
        buf
    }

    fn duration_value(seconds: i32) -> Vec<u8> {
        let mut value = Vec::with_capacity(DURATION_VALUE_LEN);
        value.extend_from_slice(&seconds.to_le_bytes());
        value.extend_from_slice(&0u32.to_le_bytes());
        value
    }

    fn announcement() -> Vec<u8> {
        let mut payload = vec![0x00, 0x03, 0x00, 0x00];
        payload.extend_from_slice(&parameter(PID_DOMAIN_ID, &7u32.to_le_bytes()));
        payload.extend_from_slice(&parameter(PID_PARTICIPANT_LEASE_DURATION, &duration_value(40)));
        payload.extend_from_slice(&parameter(
            PID_METATRAFFIC_UNICAST_LOCATOR,
            &locator_value(Ipv4Addr::new(10, 0, 0, 5), 7410),
        ));
        payload.extend_from_slice(&parameter(
            PID_METATRAFFIC_UNICAST_LOCATOR,
            &locator_value(Ipv4Addr::new(192, 168, 0, 5), 7410),
        ));
        payload.extend_from_slice(&parameter(
            PID_DEFAULT_UNICAST_LOCATOR,
            &locator_value(Ipv4Addr::new(10, 0, 0, 5), 7411),
        ));
        payload.extend_from_slice(&parameter(
            PID_METATRAFFIC_MULTICAST_LOCATOR,
            &locator_value(Ipv4Addr::new(239, 255, 0, 1), 7400),
        ));
        payload.extend_from_slice(&parameter(PID_SENTINEL, &[]));
        payload
    }

    #[test]
    fn reads_first_unicast_locator_of_each_kind() {
        let endpoints = read_endpoints(&announcement()).expect("no endpoints");
        assert_eq!(endpoints.metatraffic, SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 5), 7410));
        assert_eq!(endpoints.user_data, SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 5), 7411));
    }

    fn endpoint_announcement() -> Vec<u8> {
        let mut payload = vec![0x00, 0x03, 0x00, 0x00];
        payload.extend_from_slice(&parameter(0x005A, &[7u8; 16])); // endpoint guid
        payload.extend_from_slice(&parameter(
            PID_UNICAST_LOCATOR,
            &locator_value(Ipv4Addr::new(10, 0, 0, 5), 7411),
        ));
        payload.extend_from_slice(&parameter(
            PID_UNICAST_LOCATOR,
            &locator_value(Ipv4Addr::new(192, 168, 0, 5), 7411),
        ));
        payload.extend_from_slice(&parameter(
            PID_MULTICAST_LOCATOR,
            &locator_value(Ipv4Addr::new(239, 255, 0, 1), 7401),
        ));
        payload.extend_from_slice(&parameter(PID_SENTINEL, &[]));
        payload
    }

    #[test]
    fn stripping_leaves_an_endpoint_without_an_address_of_its_own() {
        let mut payload = endpoint_announcement();
        let before = payload.len();

        strip_endpoint_locators(&mut payload);
        assert_eq!(payload.len(), before, "stripping must not resize the payload");

        let parameters = parse(&payload, true);
        assert!(!parameters
            .iter()
            .any(|p| matches!(p.id, PID_UNICAST_LOCATOR | PID_MULTICAST_LOCATOR)));
        assert_eq!(parameters.iter().filter(|p| p.id == PID_PAD).count(), 3);
        assert!(parameters.iter().any(|p| p.id == 0x005A), "the endpoint guid must survive");
    }

    #[test]
    fn reads_the_announced_lease() {
        assert_eq!(read_lease(&announcement()), Some(Duration::from_secs(40)));
    }

    #[test]
    fn lease_is_absent_when_the_announcement_omits_it() {
        let mut payload = vec![0x00, 0x03, 0x00, 0x00];
        payload.extend_from_slice(&parameter(PID_DOMAIN_ID, &1u32.to_le_bytes()));
        payload.extend_from_slice(&parameter(PID_SENTINEL, &[]));

        assert_eq!(read_lease(&payload), None);
    }

    #[test]
    fn rewrite_leaves_the_lease_alone() {
        let mut payload = announcement();
        assert!(rewrite(&mut payload, Ipv4Addr::new(172, 16, 0, 1), 7500, 7501, 3));
        assert_eq!(read_lease(&payload), Some(Duration::from_secs(40)));
    }

    #[test]
    fn rewrite_redirects_every_address_to_the_gateway() {
        let mut payload = announcement();
        let before = payload.len();

        assert!(rewrite(&mut payload, Ipv4Addr::new(172, 16, 0, 1), 7500, 7501, 3));
        assert_eq!(payload.len(), before, "rewrite must not resize the payload");

        let endpoints = read_endpoints(&payload).expect("no endpoints");
        assert_eq!(endpoints.metatraffic, SocketAddrV4::new(Ipv4Addr::new(172, 16, 0, 1), 7500));
        assert_eq!(endpoints.user_data, SocketAddrV4::new(Ipv4Addr::new(172, 16, 0, 1), 7501));

        let parameters = parse(&payload, true);
        let domain = parameters.iter().find(|p| p.id == PID_DOMAIN_ID).expect("no domain id");
        assert_eq!(read_u32(&payload, domain.value_offset, true), 3);
    }

    #[test]
    fn rewrite_pads_out_extra_and_multicast_locators() {
        let mut payload = announcement();
        assert!(rewrite(&mut payload, Ipv4Addr::new(172, 16, 0, 1), 7500, 7501, 3));

        let parameters = parse(&payload, true);
        assert_eq!(
            parameters.iter().filter(|p| p.id == PID_METATRAFFIC_UNICAST_LOCATOR).count(),
            1
        );
        assert!(!parameters.iter().any(|p| p.id == PID_METATRAFFIC_MULTICAST_LOCATOR));
        assert_eq!(parameters.iter().filter(|p| p.id == PID_PAD).count(), 2);
    }

    #[test]
    fn rewrite_reports_failure_without_both_unicast_locators() {
        let mut payload = vec![0x00, 0x03, 0x00, 0x00];
        payload.extend_from_slice(&parameter(PID_DOMAIN_ID, &1u32.to_le_bytes()));
        payload.extend_from_slice(&parameter(PID_SENTINEL, &[]));

        assert!(!rewrite(&mut payload, Ipv4Addr::new(127, 0, 0, 1), 1, 2, 0));
        assert!(read_endpoints(&payload).is_none());
    }
}
