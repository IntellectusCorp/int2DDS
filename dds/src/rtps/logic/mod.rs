//! # RTPS Protocol Logic
//!
//! This module implements the core RTPS protocol state machines and logic.
//!
//! ## Overview
//!
//! The logic module contains the protocol behavior for discovery, endpoint
//! matching, and data exchange.
//!
//! ## Submodules
//!
//! - [`data`] - Data handling and serialization logic
//! - [`sedp_logic`] - SEDP (Simple Endpoint Discovery Protocol) logic
//! - [`spdp_logic`] - SPDP (Simple Participant Discovery Protocol) logic
//! - [`user_logic`] - User data exchange logic
//! - [`wlp_logic`] - Writer Liveliness Protocol logic

pub(crate) mod data;
pub(crate) mod message_processor;
pub(crate) mod sedp_logic;
pub(crate) mod spdp_logic;
pub(crate) mod user_logic;
pub(crate) mod wlp_logic;
