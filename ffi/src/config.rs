//! # XML Configuration (QoS profiles + participant tree)
//!
//! C-callable surface over the factory's XML config APIs, mirroring the Rust
//! `DomainParticipantFactory` methods:
//!
//! - [`int2dds_load_profiles`] wraps `load_profiles` — loads QoS profiles (and,
//!   for XML files, the `<types>`) into the factory singleton.
//! - [`int2dds_get_dynamic_type_support`] wraps `get_dynamic_type_support` —
//!   builds a dynamic type support for a type declared in a loaded `<types>`.
//! - [`int2dds_create_participant_from_config`] wraps
//!   `create_participant_from_config` — builds an entire participant tree
//!   (participant + publishers/subscribers + datawriters/datareaders + topics)
//!   from a `<domain_participant_library>` declaration. The resulting endpoints
//!   carry `DynamicData` and are addressable by their XML name.
//!
//! Load the XML with [`int2dds_load_profiles`] first, then either drive the
//! whole tree with [`int2dds_create_participant_from_config`], or fetch a type
//! support with [`int2dds_get_dynamic_type_support`] and build endpoints by hand.

use std::ffi::CStr;
use std::os::raw::c_char;
use std::path::PathBuf;
use std::sync::Arc;

use int2dds::domain::domain_participant_factory::{
    ConfiguredParticipant, DomainParticipantFactory,
};

use crate::dynamic::{
    Int2DdsDynamicDataReader, Int2DdsDynamicDataWriter, Int2DdsDynamicTypeSupport,
};
use crate::error::*;
use crate::types::Int2DdsParticipantFactory;

/// Opaque handle wrapping a [`ConfiguredParticipant`] — the whole tree built by
/// [`int2dds_create_participant_from_config`]. Destroy with
/// [`int2dds_configured_participant_destroy`].
pub struct Int2DdsConfiguredParticipant {
    inner: ConfiguredParticipant,
}
unsafe impl Send for Int2DdsConfiguredParticipant {}
unsafe impl Sync for Int2DdsConfiguredParticipant {}

/// Load QoS profiles (and, for XML files, the `<types>` section) from one or
/// more files into the factory singleton. Profiles loaded here can then be used
/// with the `*_with_profile` creators and with
/// [`int2dds_create_participant_from_config`]; types can be fetched with
/// [`int2dds_get_dynamic_type_support`].
///
/// # Safety
/// - `paths` must point to `count` valid, null-terminated UTF-8 C strings
/// - each element of `paths` must be non-null
#[no_mangle]
pub unsafe extern "C" fn int2dds_load_profiles(
    _factory: *const Int2DdsParticipantFactory,
    paths: *const *const c_char,
    count: usize,
) -> Int2DdsRet {
    check_null!(paths);

    let mut owned: Vec<PathBuf> = Vec::with_capacity(count);
    for i in 0..count {
        let entry = *paths.add(i);
        check_null!(entry);
        match CStr::from_ptr(entry).to_str() {
            Ok(s) => owned.push(PathBuf::from(s)),
            Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
        }
    }

    let factory = DomainParticipantFactory::get_instance();
    ffi_try!(factory.load_profiles(&owned));
    INT2DDS_RET_OK
}

/// Build a dynamic type support for a type declared in a `<types>` section that
/// was loaded via [`int2dds_load_profiles`]. Destroy the result with
/// `int2dds_dynamic_type_support_destroy`.
///
/// # Safety
/// - `type_name` must be a valid, null-terminated UTF-8 C string
/// - `out` must be a valid pointer to a null pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_get_dynamic_type_support(
    _factory: *const Int2DdsParticipantFactory,
    type_name: *const c_char,
    out: *mut *mut Int2DdsDynamicTypeSupport,
) -> Int2DdsRet {
    check_null!(type_name);
    check_null!(out);
    let name = match CStr::from_ptr(type_name).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let factory = DomainParticipantFactory::get_instance();
    let support = ffi_try!(factory.get_dynamic_type_support(name));
    *out = Box::into_raw(Box::new(Int2DdsDynamicTypeSupport { inner: Arc::new(support) }));
    INT2DDS_RET_OK
}

/// Build an entire participant tree from a `<domain_participant_library>`
/// declaration at `path` (`ParticipantLibrary::Participant`, e.g.
/// `"PL::PubApp"`). The XML must have been loaded via
/// [`int2dds_load_profiles`]. Endpoints carry `DynamicData` and are fetched by
/// their XML name with the accessors below. Destroy with
/// [`int2dds_configured_participant_destroy`].
///
/// # Safety
/// - `path` must be a valid, null-terminated UTF-8 C string
/// - `out` must be a valid pointer to a null pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_create_participant_from_config(
    _factory: *const Int2DdsParticipantFactory,
    path: *const c_char,
    out: *mut *mut Int2DdsConfiguredParticipant,
) -> Int2DdsRet {
    check_null!(path);
    check_null!(out);
    let path_str = match CStr::from_ptr(path).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let factory = DomainParticipantFactory::get_instance();
    let configured = ffi_try!(factory.create_participant_from_config(path_str));
    *out = Box::into_raw(Box::new(Int2DdsConfiguredParticipant { inner: configured }));
    INT2DDS_RET_OK
}

/// Get the datawriter declared as `"<publisher>::<writer>"` from a configured
/// tree. Returns `INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND` when no such writer
/// exists. The returned handle must be freed with `int2dds_dynamic_writer_destroy`.
///
/// # Safety
/// - `configured` must be a handle from `int2dds_create_participant_from_config`
/// - `name` must be a valid, null-terminated UTF-8 C string
/// - `out` must be a valid pointer to a null pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_configured_participant_get_datawriter(
    configured: *const Int2DdsConfiguredParticipant,
    name: *const c_char,
    out: *mut *mut Int2DdsDynamicDataWriter,
) -> Int2DdsRet {
    check_null!(configured);
    check_null!(name);
    check_null!(out);
    let name_str = match CStr::from_ptr(name).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    match (*configured).inner.datawriter(name_str) {
        Some(writer) => {
            *out = Box::into_raw(Box::new(Int2DdsDynamicDataWriter { inner: writer }));
            INT2DDS_RET_OK
        }
        None => INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND,
    }
}

/// Get the datareader declared as `"<subscriber>::<reader>"` from a configured
/// tree. Returns `INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND` when no such reader
/// exists. The returned handle must be freed with `int2dds_dynamic_reader_destroy`.
///
/// # Safety
/// - `configured` must be a handle from `int2dds_create_participant_from_config`
/// - `name` must be a valid, null-terminated UTF-8 C string
/// - `out` must be a valid pointer to a null pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_configured_participant_get_datareader(
    configured: *const Int2DdsConfiguredParticipant,
    name: *const c_char,
    out: *mut *mut Int2DdsDynamicDataReader,
) -> Int2DdsRet {
    check_null!(configured);
    check_null!(name);
    check_null!(out);
    let name_str = match CStr::from_ptr(name).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    match (*configured).inner.datareader(name_str) {
        Some(reader) => {
            *out = Box::into_raw(Box::new(Int2DdsDynamicDataReader { inner: reader }));
            INT2DDS_RET_OK
        }
        None => INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND,
    }
}

/// Destroy a configured-participant handle and fully tear down its tree. Safe to
/// call with null. This drops the tree's owned publishers/subscribers/topics,
/// then deletes the participant's contained entities and removes the participant
/// from the factory (equivalent to `delete_contained_entities` +
/// `delete_participant`). Destroy any datawriter/datareader handles obtained via
/// the accessors above BEFORE calling this.
///
/// # Safety
/// - `configured` must be null or a handle from
///   `int2dds_create_participant_from_config`, not used after this call
#[no_mangle]
pub unsafe extern "C" fn int2dds_configured_participant_destroy(
    configured: *mut Int2DdsConfiguredParticipant,
) {
    if configured.is_null() {
        return;
    }
    let boxed = Box::from_raw(configured);
    let participant = boxed.inner.participant.clone();
    // Release the tree's owned publishers/subscribers/datawriters/datareaders/topics.
    drop(boxed);
    // Then tear down the participant itself, mirroring the Rust cleanup path.
    let _ = participant.delete_contained_entities();
    let _ = DomainParticipantFactory::get_instance().delete_participant(participant);
}
