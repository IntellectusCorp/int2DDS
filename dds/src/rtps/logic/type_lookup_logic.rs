//! TypeLookup builtin service logic (DDS-XTypes 1.3 §7.6.3).
//!
//! Implemented as additional methods on [`SedpLogic`] because TypeLookup
//! samples travel over the metatraffic channel and reuse SEDP's DATA send
//! path ([`SedpLogic::send_sedp_data_message`]) and reception bookkeeping.
//!
//! Roles (every participant plays both):
//! - replier: answers `getTypes`/`getTypeDependencies` from its
//!   [`TypeRegistry`], returning the transitive closure it can resolve.
//! - requester: on discovering a `TypeIdentifier` whose `TypeObject` is not
//!   yet local, sends `getTypes` and registers whatever the reply carries.

use std::sync::Arc;

use log::{debug, warn};

use crate::{
    common::instance_handle::InstanceHandle,
    rtps::{
        common::{
            entity_id::EntityId,
            guid::{Guid, GuidPrefix},
            rtps_error_code::RtpsResult,
            time::RtpsTime,
            types::ChangeKind,
        },
        entities::writer::Writer as _,
        logic::{common::ParticipantAccessor as _, sedp_logic::SedpLogic},
        messages::{header::Header, submessages::data::Data},
    },
    xtypes::{
        GetTypeDependenciesOut, GetTypesOut, ReplyHeader, RequestHeader, SampleIdentity,
        TypeLookupCall, TypeLookupReply, TypeLookupRequest, TypeLookupReturn,
    },
};

use crate::xtypes::{
    GetTypeDependenciesIn, GetTypesIn, TypeIdentifier, TypeIdentifierWithSize, TypeObject,
};

impl SedpLogic {
    /// True for the two TypeLookup writer entity ids whose samples this logic
    /// owns (request from a remote requester, reply from a remote replier).
    pub(crate) fn is_type_lookup_data(data: &Data) -> bool {
        data.writer_id == EntityId::TYPE_LOOKUP_REQUEST_WRITER
            || data.writer_id == EntityId::TYPE_LOOKUP_REPLY_WRITER
    }

    /// Entry point from `handle_data_message` for TypeLookup samples. Marks the
    /// change received on the matching reliable reader, then dispatches.
    pub(crate) fn handle_type_lookup_data(
        &self,
        rtps_header: &Header,
        data: &Data,
    ) -> RtpsResult<()> {
        let participant = self.get_upgraded_participant()?;
        let remote_prefix = rtps_header.guid_prefix();
        let writer_guid = Guid::new(remote_prefix, data.writer_id);

        let reader = if data.writer_id == EntityId::TYPE_LOOKUP_REQUEST_WRITER {
            participant.type_lookup_request_reader()
        } else {
            participant.type_lookup_reply_reader()
        };
        let _ = self.mark_as_received_in_writer_proxy(&reader, writer_guid, data.writer_sn);

        let payload = data.serialized_data();
        if data.writer_id == EntityId::TYPE_LOOKUP_REQUEST_WRITER {
            match TypeLookupRequest::deserialize(payload.as_ref()) {
                Ok(request) => self.handle_type_lookup_request(remote_prefix, request),
                Err(e) => {
                    warn!("[TypeLookup] failed to parse request: {}", e);
                    Ok(())
                }
            }
        } else {
            match TypeLookupReply::deserialize(payload.as_ref()) {
                Ok(reply) => self.handle_type_lookup_reply(reply),
                Err(e) => {
                    warn!("[TypeLookup] failed to parse reply: {}", e);
                    Ok(())
                }
            }
        }
    }

    fn handle_type_lookup_request(
        &self,
        requester_prefix: GuidPrefix,
        request: TypeLookupRequest,
    ) -> RtpsResult<()> {
        let ret = match &request.data {
            TypeLookupCall::GetTypes(GetTypesIn { type_ids }) => {
                TypeLookupReturn::GetTypes(self.serve_get_types(type_ids))
            }
            TypeLookupCall::GetTypeDependencies(GetTypeDependenciesIn { type_ids, .. }) => {
                TypeLookupReturn::GetTypeDependencies(self.serve_get_type_dependencies(type_ids))
            }
        };

        let reply = TypeLookupReply {
            header: ReplyHeader {
                related_request_id: request.header.request_id,
                remote_exception_code: 0,
            },
            data: ret,
        };

        let remote_reader_guid = Guid::new(requester_prefix, EntityId::TYPE_LOOKUP_REPLY_READER);
        self.send_type_lookup_sample(
            &self.get_upgraded_participant()?.type_lookup_reply_writer(),
            remote_reader_guid,
            EntityId::TYPE_LOOKUP_REPLY_READER,
            EntityId::TYPE_LOOKUP_REPLY_WRITER,
            reply.serialize(),
        )
    }

    fn handle_type_lookup_reply(&self, reply: TypeLookupReply) -> RtpsResult<()> {
        let participant = self.get_upgraded_participant()?;
        match reply.data {
            TypeLookupReturn::GetTypes(GetTypesOut { types }) => {
                let mut still_missing: Vec<TypeIdentifier> = Vec::new();
                if let Ok(mut registry) = participant.type_registry().write() {
                    for (type_id, type_object) in &types {
                        registry.register_type_object_with_id(type_id, type_object.clone());
                    }
                    for (type_id, _) in &types {
                        if let Some(hash) = type_id.equivalence_hash() {
                            for dep in registry.missing_dependencies(hash) {
                                still_missing.push(TypeIdentifier::CompleteTypeId(dep));
                            }
                        }
                    }
                }
                debug!(
                    "[TypeLookup] registered {} type object(s), {} still missing",
                    types.len(),
                    still_missing.len()
                );
                if !still_missing.is_empty() {
                    let prefix = reply.header.related_request_id.writer_guid.prefix();
                    let _ = self.request_get_types(prefix, still_missing);
                }
                Ok(())
            }
            TypeLookupReturn::GetTypeDependencies(GetTypeDependenciesOut {
                dependent_typeids,
                ..
            }) => {
                let type_ids = dependent_typeids.into_iter().map(|d| d.type_id).collect::<Vec<_>>();
                if !type_ids.is_empty() {
                    let prefix = reply.header.related_request_id.writer_guid.prefix();
                    let _ = self.request_get_types(prefix, type_ids);
                }
                Ok(())
            }
        }
    }

    /// Replier helper: resolve each requested id to its complete closure.
    fn serve_get_types(&self, type_ids: &[TypeIdentifier]) -> GetTypesOut {
        let mut types: Vec<(TypeIdentifier, TypeObject)> = Vec::new();
        if let Ok(participant) = self.get_upgraded_participant() {
            if let Ok(registry) = participant.type_registry().read() {
                for type_id in type_ids {
                    if let Some(hash) = type_id.equivalence_hash() {
                        for (h, obj) in registry.complete_closure(hash) {
                            types.push((
                                TypeIdentifier::CompleteTypeId(h),
                                TypeObject::Complete(obj),
                            ));
                        }
                    }
                }
            }
        }
        GetTypesOut { types }
    }

    /// Replier helper: list each requested id's direct dependencies (with sizes).
    fn serve_get_type_dependencies(&self, type_ids: &[TypeIdentifier]) -> GetTypeDependenciesOut {
        let mut dependent_typeids: Vec<TypeIdentifierWithSize> = Vec::new();
        if let Ok(participant) = self.get_upgraded_participant() {
            if let Ok(registry) = participant.type_registry().read() {
                for type_id in type_ids {
                    if let Some(hash) = type_id.equivalence_hash() {
                        if let Some(deps) = registry.get_dependencies(hash) {
                            for dep in deps {
                                let size = registry
                                    .lookup_complete(dep)
                                    .map(|o| {
                                        TypeObject::Complete(o.clone()).serialize().len() as u32
                                    })
                                    .unwrap_or(0);
                                dependent_typeids.push(TypeIdentifierWithSize::new(
                                    TypeIdentifier::CompleteTypeId(*dep),
                                    size,
                                ));
                            }
                        }
                    }
                }
            }
        }
        GetTypeDependenciesOut { dependent_typeids, continuation_point: Vec::new() }
    }

    /// Requester: send a `getTypes` request for `type_ids` to `remote_prefix`.
    /// No-op when there is nothing to ask for.
    pub(crate) fn request_get_types(
        &self,
        remote_prefix: GuidPrefix,
        type_ids: Vec<TypeIdentifier>,
    ) -> RtpsResult<()> {
        if type_ids.is_empty() {
            return Ok(());
        }
        let participant = self.get_upgraded_participant()?;
        let writer = participant.type_lookup_request_writer();
        let call = TypeLookupCall::GetTypes(GetTypesIn { type_ids });

        let change = writer.new_change_with_rpc_callback(
            ChangeKind::Alive,
            InstanceHandle::NIL,
            Some(RtpsTime::now()),
            Box::new(move |writer_guid, seq| {
                let request = TypeLookupRequest {
                    header: RequestHeader {
                        request_id: SampleIdentity::new(writer_guid, seq),
                        instance_name: String::new(),
                    },
                    data: call,
                };
                request.serialize()
            }),
        );
        let cache_change = Arc::new(change);
        if let Ok(mut cache) = writer.writer_cache().lock() {
            let _ = cache.add_change_builtin(cache_change.clone());
        }

        let remote_reader_guid = Guid::new(remote_prefix, EntityId::TYPE_LOOKUP_REQUEST_READER);
        self.send_type_lookup_sample_change(
            remote_reader_guid,
            EntityId::TYPE_LOOKUP_REQUEST_READER,
            EntityId::TYPE_LOOKUP_REQUEST_WRITER,
            cache_change,
        )
    }

    pub(crate) fn maybe_request_discovered_type(
        &self,
        remote_prefix: GuidPrefix,
        type_identifier: Option<&TypeIdentifier>,
        has_inline_object: bool,
    ) {
        if has_inline_object {
            return;
        }
        let Some(type_id) = type_identifier else { return };
        let Some(hash) = type_id.equivalence_hash() else { return };
        let already = self
            .get_upgraded_participant()
            .ok()
            .and_then(|p| p.type_registry().read().ok().map(|r| r.contains(hash)))
            .unwrap_or(false);
        if already {
            return;
        }
        let _ = self.request_get_types(remote_prefix, vec![type_id.clone()]);
    }

    /// Build a CacheChange for `payload`, track it on `writer`, and send it.
    fn send_type_lookup_sample(
        &self,
        writer: &Arc<crate::rtps::entities::writer::StatefulWriter>,
        remote_reader_guid: Guid,
        reader_entity_id: EntityId,
        writer_entity_id: EntityId,
        payload: Vec<u8>,
    ) -> RtpsResult<()> {
        let change = writer.new_change(
            ChangeKind::Alive,
            payload,
            InstanceHandle::NIL,
            Some(RtpsTime::now()),
        );
        let cache_change = Arc::new(change);
        if let Ok(mut cache) = writer.writer_cache().lock() {
            let _ = cache.add_change_builtin(cache_change.clone());
        }
        self.send_type_lookup_sample_change(
            remote_reader_guid,
            reader_entity_id,
            writer_entity_id,
            cache_change,
        )
    }

    /// Send an already-cached TypeLookup CacheChange over the metatraffic path.
    fn send_type_lookup_sample_change(
        &self,
        remote_reader_guid: Guid,
        reader_entity_id: EntityId,
        writer_entity_id: EntityId,
        cache_change: Arc<crate::rtps::entities::history::cache_change::CacheChange>,
    ) -> RtpsResult<()> {
        self.send_sedp_data_message(
            cache_change,
            remote_reader_guid,
            reader_entity_id,
            writer_entity_id,
        )
    }
}
