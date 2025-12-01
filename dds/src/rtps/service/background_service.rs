//! Background service for signal handling and graceful termination.
//!
//! This module provides the background service that monitors system signals
//! (SIGTERM, SIGINT on Unix and Ctrl+C on Windows) and handles graceful shutdown
//! of RTPS participants when termination signals are received.

use crate::rtps::dcps_bridge::dcps_bridge::PARTICIPANTS;
use crate::rtps::task::sending_handler::SendingHandler;
use log::{error, info};
use std::thread::JoinHandle;
use std::{
    sync::{Arc, Mutex},
    thread,
};

#[cfg(unix)]
use signal_hook::{
    consts::{SIGINT, SIGTERM},
    iterator::Signals,
};

#[cfg(windows)]
use ctrlc;

#[derive(Clone)]
pub struct BackgroundService {
    background_thread_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl Default for BackgroundService {
    fn default() -> Self {
        Self::new()
    }
}

impl BackgroundService {
    pub(crate) fn new() -> Self {
        Self { background_thread_handle: Arc::new(Mutex::new(None)) }
    }

    pub(crate) fn start_background_thread(&self) {
        let handler = thread::Builder::new()
            .name("background thread".to_string())
            .spawn(move || {
                // Register thread name for monitoring
                {
                    use crate::rtps::task::thread_monitor::ThreadMonitor;
                    ThreadMonitor::register_current_thread_name("background thread");
                }
                #[cfg(unix)]
                {
                    let mut signals = Signals::new([SIGINT, SIGTERM]).unwrap();

                    loop {
                        if let Some(sig) = signals.pending().next() {
                            Self::termination_on_signal_receival(sig.to_string());
                            return;
                        }

                        // Wait briefly then check again
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                }

                #[cfg(windows)]
                {
                    ctrlc::set_handler(move || {
                        Self::termination_on_signal_receival("Ctrl+C".to_string());
                    })
                    .expect("Error setting Ctrl-C handler");

                    // Infinite wait on Windows
                    loop {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                }
            })
            .unwrap();

        // Store background handle
        if let Ok(mut handle_guard) = self.background_thread_handle.lock() {
            *handle_guard = Some(handler);
        }
    }

    pub(crate) fn termination_on_signal_receival(signal: String) {
        info!("Received signal {:?}, shutting down...", signal);

        thread::scope(|s| {
            PARTICIPANTS.read().unwrap().iter().for_each(|participant| {
                if let Some(participant) = participant.upgrade() {
                    s.spawn(move || {
                        participant.terminate();

                        let sending_handler =
                            SendingHandler::get_instance(Arc::clone(&participant), None, None);

                        sending_handler.wake_event_loop();
                        if let Err(e) = sending_handler.join_sending_thread() {
                            error!("Failed to join sending thread: {}", e);
                        }

                        // Send termination message over network synchronously
                        let _ = participant.send_termination_message_on_shutdown();
                    });
                }
            });
        });

        std::process::exit(0);
    }
}
