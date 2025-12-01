//! # RTPS Tasks
//!
//! This module implements the task-based architecture for RTPS operations.
//!
//! ## Overview
//!
//! Tasks handle specific aspects of RTPS communication, including network I/O,
//! timer management, and message sending. Each task runs in its own thread
//! for concurrent operation.
//!
//! ## Submodules
//!
//! - [`discovery_traffic`] - Discovery message handling (SPDP/SEDP)
//! - [`sending_handler`] - Message sending coordination
//! - [`sending_task`] - Dedicated sending thread with thread pool
//! - [`thread_monitor`] - Thread monitoring and diagnostics
//! - [`timer_handler`] - Timer event handling
//! - [`timer_task`] - Timer management task
//! - [`user_traffic`] - User data message handling

pub(crate) mod discovery_traffic;
pub(crate) mod sending_handler;
pub(crate) mod sending_task;
pub(crate) mod thread_monitor;
pub(crate) mod timer_handler;
pub(crate) mod timer_task;
pub(crate) mod user_traffic;
