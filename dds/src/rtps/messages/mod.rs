//! # RTPS Messages
//!
//! This module implements RTPS message structures, serialization, and processing.
//!
//! ## Overview
//!
//! RTPS messages consist of a header followed by one or more submessages.
//! This module provides types and functions for creating, parsing, and handling
//! RTPS messages according to the RTPS specification.
//!
//! ## Message Structure
//!
//! ```text
//! ┌─────────────────┐
//! │   RTPS Header   │
//! ├─────────────────┤
//! │  Submessage 1   │
//! ├─────────────────┤
//! │  Submessage 2   │
//! ├─────────────────┤
//! │      ...        │
//! └─────────────────┘
//! ```
//!
//! ## Submodules
//!
//! - [`header`] - RTPS message header
//! - [`header_extension`] - Header extension for additional metadata
//! - [`message_creator`] - Message construction utilities
//! - [`message_receiver`] - Message parsing and processing
//! - [`rtps_messages`] - Complete RTPS message type
//! - [`sedp_message`] - SEDP-specific message handling
//! - [`spdp_message`] - SPDP-specific message handling
//! - [`submessage`] - Submessage structure
//! - [`submessage_body`] - Submessage body contents
//! - [`submessage_creator`] - Submessage construction utilities
//! - [`submessage_data_participant`] - Participant data submessage
//! - [`submessage_header`] - Submessage header
//! - [`submessage_header_flag`] - Submessage header flags
//! - [`submessage_id`] - Submessage type identifiers
//! - [`submessages`] - Submessage type definitions

pub(crate) mod header;
pub(crate) mod header_extension;
pub(crate) mod message_creator;
pub(crate) mod message_receiver;
pub(crate) mod rtps_messages;
pub(crate) mod sedp_message;
pub(crate) mod spdp_message;
pub(crate) mod submessage;
pub(crate) mod submessage_body;
pub(crate) mod submessage_creator;
pub(crate) mod submessage_data_participant;
pub(crate) mod submessage_header;
pub(crate) mod submessage_header_flag;
pub(crate) mod submessage_id;
pub(crate) mod submessages;
