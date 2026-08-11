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
            sequence::SequenceNumber,
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
    chunk_dependencies, GetTypeDependenciesIn, GetTypesIn, TypeIdentifier, TypeIdentifierWithSize,
    TypeObject,
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
                Ok(reply) => self.handle_type_lookup_reply(remote_prefix, reply),
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
            TypeLookupCall::GetTypeDependencies(GetTypeDependenciesIn {
                type_ids,
                continuation_point,
            }) => TypeLookupReturn::GetTypeDependencies(
                self.serve_get_type_dependencies(type_ids, continuation_point),
            ),
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

    fn handle_type_lookup_reply(
        &self,
        replier_prefix: GuidPrefix,
        reply: TypeLookupReply,
    ) -> RtpsResult<()> {
        let participant = self.get_upgraded_participant()?;
        match reply.data {
            TypeLookupReturn::GetTypes(GetTypesOut { types, complete_to_minimal }) => {
                let related = &reply.header.related_request_id;
                let pending =
                    self.type_lookup_pending.lock().ok().and_then(|mut map| map.remove(related));
                if pending.is_none() {
                    warn!(
                        "[TypeLookup] getTypes reply with no matching outstanding request; dropping"
                    );
                    return Ok(());
                }
                let prefix = pending.map(|(p, _)| p).unwrap_or(replier_prefix);

                let mut still_missing: Vec<TypeIdentifier> = Vec::new();
                if let Ok(mut registry) = participant.type_registry().write() {
                    for (type_id, type_object) in &types {
                        registry.register_type_object_with_id(type_id, type_object.clone());
                    }

                    let batch_hashes: Vec<_> =
                        types.iter().filter_map(|(id, _)| id.equivalence_hash().copied()).collect();
                    loop {
                        let mut progressed = false;
                        for h in &batch_hashes {
                            if registry.minimal_hash_of(h).is_none()
                                && registry.try_derive_minimal(h)
                            {
                                progressed = true;
                            }
                        }
                        if !progressed {
                            break;
                        }
                    }
                    // Preserve any COMPLETE->MINIMAL correspondences the replier sent
                    // (e.g. COMPLETE served for a MINIMAL request); ours take precedence.
                    for (complete, minimal) in &complete_to_minimal {
                        if let (Some(c), Some(m)) =
                            (complete.equivalence_hash(), minimal.equivalence_hash())
                        {
                            registry.note_complete_to_minimal(*c, *m);
                        }
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
                    let _ = self.request_get_types(prefix, still_missing);
                }
                self.re_match_resolved_types(prefix);
                Ok(())
            }
            TypeLookupReturn::GetTypeDependencies(GetTypeDependenciesOut {
                dependent_typeids,
                continuation_point,
            }) => {
                let related = &reply.header.related_request_id;
                let pending =
                    self.type_lookup_pending.lock().ok().and_then(|mut map| map.remove(related));
                let prefix = pending.as_ref().map(|(p, _)| *p).unwrap_or(replier_prefix);

                let mut needed: Vec<TypeIdentifier> = Vec::new();
                if let Ok(registry) = participant.type_registry().read() {
                    for dep in &dependent_typeids {
                        let unknown = dep
                            .type_id
                            .equivalence_hash()
                            .map(|h| !registry.contains(h))
                            .unwrap_or(true);
                        if unknown && !needed.contains(&dep.type_id) {
                            needed.push(dep.type_id.clone());
                        }
                    }
                }

                if continuation_point.is_empty() {
                    // All dependencies enumerated: fetch them plus the root itself.
                    if let Some((_, root)) = pending {
                        if !needed.contains(&root) {
                            needed.push(root);
                        }
                    }
                } else if let Some((_, root)) = pending {
                    // More dependencies remain: page the next round for the same root.
                    let _ = self.request_get_type_dependencies(prefix, root, continuation_point);
                }

                let _ = self.request_get_types(prefix, needed);
                Ok(())
            }
        }
    }

    /// Replier helper: resolve each requested id to its closure, served in the
    /// equivalence kind the request asked for (MINIMAL for a `MinimalTypeId`,
    /// COMPLETE for a `CompleteTypeId`; spec §7.6.3 allows serving MINIMAL directly).
    fn serve_get_types(&self, type_ids: &[TypeIdentifier]) -> GetTypesOut {
        let mut types: Vec<(TypeIdentifier, TypeObject)> = Vec::new();
        let mut complete_to_minimal: Vec<(TypeIdentifier, TypeIdentifier)> = Vec::new();
        if let Ok(participant) = self.get_upgraded_participant() {
            if let Ok(registry) = participant.type_registry().read() {
                let mut served = std::collections::HashSet::new();
                for type_id in type_ids {
                    let want_minimal = matches!(type_id, TypeIdentifier::MinimalTypeId(_));
                    let hash = match type_id.equivalence_hash() {
                        Some(h) => *h,
                        None => continue,
                    };
                    // A minimal-only registry (no complete mapping): serve it directly.
                    if want_minimal && registry.complete_hash_of(&hash).is_none() {
                        if registry.lookup_complete(&hash).is_none() {
                            if let Some(obj) = registry.resolve_type_object(type_id) {
                                if served.insert(hash) {
                                    types.push((type_id.clone(), obj));
                                }
                            }
                            continue;
                        }
                    }
                    let complete_root = if want_minimal {
                        registry.complete_hash_of(&hash).unwrap_or(hash)
                    } else {
                        hash
                    };
                    let mut closure = vec![complete_root];
                    closure.extend(registry.transitive_dependency_hashes(&[complete_root]));
                    for ch in closure {
                        let served_id = if want_minimal {
                            match registry.minimal_hash_of(&ch) {
                                Some(mh) => TypeIdentifier::MinimalTypeId(mh),
                                None => continue,
                            }
                        } else {
                            TypeIdentifier::CompleteTypeId(ch)
                        };
                        if let Some(k) = served_id.equivalence_hash().copied() {
                            if !served.insert(k) {
                                continue;
                            }
                        }
                        if let Some(obj) = registry.resolve_type_object(&served_id) {
                            if let TypeIdentifier::CompleteTypeId(ch) = &served_id {
                                if let Some(mh) = registry.minimal_hash_of(ch) {
                                    complete_to_minimal.push((
                                        served_id.clone(),
                                        TypeIdentifier::MinimalTypeId(mh),
                                    ));
                                }
                            }
                            types.push((served_id, obj));
                        }
                    }
                }
            }
        }
        GetTypesOut { types, complete_to_minimal }
    }

    fn serve_get_type_dependencies(
        &self,
        type_ids: &[TypeIdentifier],
        continuation_point: &[u8],
    ) -> GetTypeDependenciesOut {
        let mut dependent_typeids: Vec<TypeIdentifierWithSize> = Vec::new();
        if let Ok(participant) = self.get_upgraded_participant() {
            if let Ok(registry) = participant.type_registry().read() {
                let want_minimal = type_ids
                    .first()
                    .map_or(false, |id| matches!(id, TypeIdentifier::MinimalTypeId(_)));
                let roots: Vec<_> =
                    type_ids
                        .iter()
                        .filter_map(|id| id.equivalence_hash().copied())
                        .map(|h| {
                            if want_minimal {
                                registry.complete_hash_of(&h).unwrap_or(h)
                            } else {
                                h
                            }
                        })
                        .collect();
                for dep in registry.transitive_dependency_hashes(&roots) {
                    // Reply in the request's EK space; size is the spec byte length of
                    // the object actually addressed.
                    let (dep_id, obj) = if want_minimal {
                        match registry
                            .minimal_hash_of(&dep)
                            .and_then(|mh| registry.lookup_minimal(&mh).map(|m| (mh, m.clone())))
                        {
                            Some((mh, m)) => {
                                (TypeIdentifier::MinimalTypeId(mh), TypeObject::Minimal(m))
                            }
                            None => continue,
                        }
                    } else {
                        match registry.lookup_complete(&dep) {
                            Some(c) => (
                                TypeIdentifier::CompleteTypeId(dep),
                                TypeObject::Complete(c.clone()),
                            ),
                            None => continue,
                        }
                    };
                    let size = crate::xtypes::serialize_type_object(&obj).len() as u32;
                    dependent_typeids.push(TypeIdentifierWithSize::new(dep_id, size));
                }
            }
        }
        chunk_dependencies(dependent_typeids, continuation_point)
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
        let pending = self.type_lookup_pending.clone();
        let root = type_ids[0].clone();
        let call = TypeLookupCall::GetTypes(GetTypesIn { type_ids });
        self.send_type_lookup_request(
            remote_prefix,
            Box::new(move |writer_guid, seq| {
                let request_id = SampleIdentity::new(writer_guid, seq);
                if let Ok(mut map) = pending.lock() {
                    map.insert(request_id.clone(), (remote_prefix, root));
                }
                let request = TypeLookupRequest {
                    header: RequestHeader { request_id, instance_name: String::new() },
                    data: call,
                };
                request.serialize()
            }),
        )
    }

    pub(crate) fn request_get_type_dependencies(
        &self,
        remote_prefix: GuidPrefix,
        root: TypeIdentifier,
        continuation_point: Vec<u8>,
    ) -> RtpsResult<()> {
        let pending = self.type_lookup_pending.clone();
        let input = GetTypeDependenciesIn { type_ids: vec![root.clone()], continuation_point };
        self.send_type_lookup_request(
            remote_prefix,
            Box::new(move |writer_guid, seq| {
                let request_id = SampleIdentity::new(writer_guid, seq);
                if let Ok(mut map) = pending.lock() {
                    map.insert(request_id.clone(), (remote_prefix, root));
                }
                let request = TypeLookupRequest {
                    header: RequestHeader { request_id, instance_name: String::new() },
                    data: TypeLookupCall::GetTypeDependencies(input),
                };
                request.serialize()
            }),
        )
    }

    /// Build a request sample from `data_fn`, cache it on the request writer, and
    /// send it to `remote_prefix` over the metatraffic path.
    fn send_type_lookup_request<'a>(
        &self,
        remote_prefix: GuidPrefix,
        data_fn: Box<dyn FnOnce(Guid, SequenceNumber) -> Vec<u8> + 'a>,
    ) -> RtpsResult<()> {
        let participant = self.get_upgraded_participant()?;
        let writer = participant.type_lookup_request_writer();
        let change = writer.new_change_with_rpc_callback(
            ChangeKind::Alive,
            InstanceHandle::NIL,
            Some(RtpsTime::now()),
            data_fn,
        );
        let cache_change = Arc::new(change);
        if let Ok(mut cache) = writer.writer_cache().lock() {
            let _ = cache.add_change_builtin(cache_change.clone(), writer.as_ref());
        }
        let remote_reader_guid = Guid::new(remote_prefix, EntityId::TYPE_LOOKUP_REQUEST_READER);
        self.send_sedp_data_message(
            cache_change,
            remote_reader_guid,
            EntityId::TYPE_LOOKUP_REQUEST_READER,
            EntityId::TYPE_LOOKUP_REQUEST_WRITER,
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
        let _ = self.request_get_type_dependencies(remote_prefix, type_id.clone(), Vec::new());
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
            let _ = cache.add_change_builtin(cache_change.clone(), writer.as_ref());
        }
        self.send_sedp_data_message(
            cache_change,
            remote_reader_guid,
            reader_entity_id,
            writer_entity_id,
        )
    }
}
