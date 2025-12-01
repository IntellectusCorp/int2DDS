//! # Built-in Discovery Endpoints
//!
//! This module implements the RTPS built-in endpoints for participant discovery.
//!
//! ## Overview
//!
//! Built-in endpoints are pre-defined readers and writers that handle the SPDP
//! (Simple Participant Discovery Protocol) for discovering remote participants.
//!
//! ## Submodules
//!
//! - [`builtin_endpoints`] - Built-in endpoint definitions and management
//! - [`data`] - Discovery data structures (SPDPdiscoveredParticipantData)
//! - [`spdp_builtin_participant_reader`] - SPDP reader for receiving participant announcements
//! - [`spdp_builtin_participant_writer`] - SPDP writer for sending participant announcements

pub(crate) mod builtin_endpoints;
pub(crate) mod data;
pub(crate) mod spdp_builtin_participant_reader;
pub(crate) mod spdp_builtin_participant_writer;
