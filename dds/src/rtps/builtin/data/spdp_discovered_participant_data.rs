//! SPDP discovered participant data for participant discovery.
//!
//! This module defines the data structure transmitted in SPDP (Simple Participant
//! Discovery Protocol) messages, containing information about discovered domain
//! participants including their locators, available builtin endpoints, and QoS.

#![allow(dead_code)]
#![allow(unused_variables)]

use std::sync::{Arc, Mutex};

use crate::{
    common::builtin::topic::builtin_topic_key::BuiltinTopicKey,
    infrastructure::qos_policy::UserDataQosPolicy,
    rtps::common::{
        entity_id::EntityId,
        guid::{Guid, GuidPrefix},
        locator::Locator,
        rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult},
        time::RtpsDuration,
        types::{Count, DomainId, ProtocolVersion, VendorId, VENDORID_INT2},
    },
};

use super::builtin_endpoint_set::BuiltinEndpointSet;

#[derive(Debug, Clone)]
pub(crate) struct SPDPDiscoveredParticipantData {
    domain_id: DomainId,
    domain_tag: String, //topic name??

    protocol_version: ProtocolVersion,
    guid_prefix: GuidPrefix,
    participant_guid: Guid,
    vendor_id: VendorId,
    expects_inline_qos: bool,
    entity_name: String,

    available_builtin_endpoints: BuiltinEndpointSet,
    metatraffic_unicast_locator_list: Vec<Locator>,
    metatraffic_multicast_locator_list: Vec<Locator>,
    default_multicast_locator_list: Vec<Locator>,
    default_unicast_locator_list: Vec<Locator>,
    manual_liveliness_count: Arc<Mutex<Count>>,

    lease_duration: RtpsDuration,
    heartbeat_period: RtpsDuration,

    key: BuiltinTopicKey,
    user_data: UserDataQosPolicy,
    // builtin_endpoint_qos: BuiltinEndpointQos,
    // Advertised receive-buffer size (PidReceiveBufferSize). `None` means the peer
    // didn't send it, which must stay distinct from an advertised zero.
    receive_buffer_size: Option<usize>,
}

impl SPDPDiscoveredParticipantData {
    pub(crate) fn new(
        domain_id: DomainId,
        guid_prefix: GuidPrefix,
        endpointset: BuiltinEndpointSet,
    ) -> Self {
        Self {
            domain_id,
            domain_tag: String::new(),
            protocol_version: ProtocolVersion::PROTOCOLVERSION,
            guid_prefix,
            participant_guid: Guid::new(guid_prefix, EntityId::PARTICIPANT),
            vendor_id: VENDORID_INT2,
            expects_inline_qos: false,
            entity_name: String::new(),
            available_builtin_endpoints: endpointset,
            metatraffic_unicast_locator_list: Vec::new(),
            metatraffic_multicast_locator_list: Vec::new(),
            default_multicast_locator_list: Vec::new(),
            default_unicast_locator_list: Vec::new(),
            manual_liveliness_count: Arc::new(Mutex::new(0)),
            lease_duration: RtpsDuration::new(100, 0),
            heartbeat_period: RtpsDuration::new(2, 0),
            key: BuiltinTopicKey { value: [0; 3] },
            user_data: UserDataQosPolicy::default(),
            receive_buffer_size: None,
        }
    }

    pub(crate) fn domain_id(&self) -> DomainId {
        self.domain_id
    }

    pub(crate) fn domain_tag(&self) -> &str {
        &self.domain_tag
    }

    pub(crate) fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    pub(crate) fn guid_prefix(&self) -> GuidPrefix {
        self.guid_prefix
    }

    pub(crate) fn vendor_id(&self) -> VendorId {
        self.vendor_id
    }

    pub(crate) fn expects_inline_qos(&self) -> bool {
        self.expects_inline_qos
    }

    pub(crate) fn entity_name(&self) -> &str {
        &self.entity_name
    }

    pub(crate) fn set_entity_name(&mut self, entity_name: String) {
        self.entity_name = entity_name;
    }

    pub(crate) fn available_builtin_endpoints(&self) -> BuiltinEndpointSet {
        self.available_builtin_endpoints
    }

    pub(crate) fn metatraffic_unicast_locator_list(&self) -> &Vec<Locator> {
        &self.metatraffic_unicast_locator_list
    }

    pub(crate) fn metatraffic_multicast_locator_list(&self) -> &Vec<Locator> {
        &self.metatraffic_multicast_locator_list
    }

    pub(crate) fn default_multicast_locator_list(&self) -> &Vec<Locator> {
        &self.default_multicast_locator_list
    }

    pub(crate) fn default_unicast_locator_list(&self) -> &Vec<Locator> {
        &self.default_unicast_locator_list
    }

    pub(crate) fn manual_liveliness_count(&self) -> RtpsResult<Count> {
        self.manual_liveliness_count
            .lock()
            .map(|guard| *guard)
            .map_err(|e| RtpsError::new(RtpsErrorCode::LockError, e.to_string()))
    }

    pub(crate) fn lease_duration(&self) -> RtpsDuration {
        self.lease_duration
    }

    pub(crate) fn receive_buffer_size(&self) -> Option<usize> {
        self.receive_buffer_size
    }

    pub(crate) fn set_receive_buffer_size(&mut self, size: Option<usize>) {
        self.receive_buffer_size = size;
    }

    pub(crate) fn key(&self) -> &BuiltinTopicKey {
        &self.key
    }

    pub(crate) fn user_data(&self) -> &UserDataQosPolicy {
        &self.user_data
    }

    pub(crate) fn set_domain_id(&mut self, domain_id: DomainId) {
        self.domain_id = domain_id;
    }

    pub(crate) fn set_domain_tag(&mut self, domain_tag: String) {
        self.domain_tag = domain_tag;
    }

    pub(crate) fn set_protocol_version(&mut self, protocol_version: ProtocolVersion) {
        self.protocol_version = protocol_version;
    }

    pub(crate) fn set_guid_prefix(&mut self, guid_prefix: GuidPrefix) {
        self.guid_prefix = guid_prefix;
    }

    pub(crate) fn set_vendor_id(&mut self, vendor_id: VendorId) {
        self.vendor_id = vendor_id;
    }

    pub(crate) fn set_expects_inline_qos(&mut self, expects_inline_qos: bool) {
        self.expects_inline_qos = expects_inline_qos;
    }

    pub(crate) fn set_available_builtin_endpoints(&mut self, endpoints: BuiltinEndpointSet) {
        self.available_builtin_endpoints = endpoints;
    }

    pub(crate) fn set_metatraffic_unicast_locator_list(&mut self, locators: Vec<Locator>) {
        self.metatraffic_unicast_locator_list = locators;
    }

    pub(crate) fn set_metatraffic_multicast_locator_list(&mut self, locators: Vec<Locator>) {
        self.metatraffic_multicast_locator_list = locators;
    }

    pub(crate) fn set_default_multicast_locator_list(&mut self, locators: Vec<Locator>) {
        self.default_multicast_locator_list = locators;
    }

    pub(crate) fn set_default_unicast_locator_list(&mut self, locators: Vec<Locator>) {
        self.default_unicast_locator_list = locators;
    }

    pub(crate) fn set_manual_liveliness_count(&mut self, count: Count) -> RtpsResult<()> {
        *self
            .manual_liveliness_count
            .lock()
            .map_err(|e| RtpsError::new(RtpsErrorCode::LockError, e.to_string()))? = count;
        Ok(())
    }

    pub(crate) fn set_lease_duration(&mut self, duration: RtpsDuration) {
        self.lease_duration = duration;
    }

    pub(crate) fn set_key(&mut self, key: BuiltinTopicKey) {
        self.key = key;
    }

    pub(crate) fn set_user_data(&mut self, user_data: UserDataQosPolicy) {
        self.user_data = user_data;
    }

    pub(crate) fn add_metatraffic_unicast_locator(&mut self, locator: Locator) {
        self.metatraffic_unicast_locator_list.push(locator);
    }

    pub(crate) fn add_metatraffic_multicast_locator(&mut self, locator: Locator) {
        self.metatraffic_multicast_locator_list.push(locator);
    }

    pub(crate) fn add_default_multicast_locator(&mut self, locator: Locator) {
        self.default_multicast_locator_list.push(locator);
    }

    pub(crate) fn add_default_unicast_locator(&mut self, locator: Locator) {
        self.default_unicast_locator_list.push(locator);
    }

    pub(crate) fn set_participant_guid(&mut self, guid: Guid) {
        self.participant_guid = guid;
    }

    pub(crate) fn participant_guid(&self) -> Guid {
        self.participant_guid
    }

    pub(crate) fn heartbeat_period(&self) -> RtpsDuration {
        self.heartbeat_period
    }

    pub(crate) fn set_heartbeat_period(&mut self, duration: RtpsDuration) {
        self.heartbeat_period = duration;
    }

    pub(crate) fn increase_manual_liveliness_count(&self) -> RtpsResult<()> {
        *self
            .manual_liveliness_count
            .lock()
            .map_err(|e| RtpsError::new(RtpsErrorCode::LockError, e.to_string()))? += 1;
        Ok(())
    }
}
