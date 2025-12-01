//! Function execution timing and tracing infrastructure.
//!
//! This module provides tracing capabilities for measuring function execution times,
//! useful for performance profiling and optimization. Controlled by the
//! `INT2DDS_FUNCTION_TIMING` environment variable.

use std::sync::Once;
use tracing_subscriber::{
    fmt::{self, format::FmtSpan},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter, Layer,
};

static INIT: Once = Once::new();

/// Initialize tracing subscriber for function timing measurements
///
/// This function sets up tracing infrastructure based on environment variables:
/// - `INT2DDS_FUNCTION_TIMING`: Enable function timing (true/false, default: false)
/// - `INT2DDS_FUNCTION_TIMING_LOG_PATH`: Log file path (default: ./function_timing.log)
///
/// The tracing system measures and logs:
/// - Function entry/exit events
/// - Execution duration for each function
/// - Hierarchical call relationships
///
/// # Examples
///
/// ```no_run
/// use int2dds::common::tracing::init_tracing;
///
/// // Enable via environment variable
/// std::env::set_var("INT2DDS_FUNCTION_TIMING", "true");
/// init_tracing();
/// ```
///
/// # Thread Safety
///
/// This function is safe to call multiple times - initialization happens only once.
pub fn init_tracing() {
    INIT.call_once(|| {
        let timing_enabled = std::env::var("INT2DDS_FUNCTION_TIMING")
            .unwrap_or_else(|_| "false".to_string())
            .to_lowercase()
            == "true";

        if !timing_enabled {
            // Tracing disabled - no initialization needed
            log::info!(
                "Function timing measurement disabled (INT2DDS_FUNCTION_TIMING not set to 'true')"
            );
            return;
        }

        log::info!("Enabling function timing measurement...");

        let log_path = std::env::var("INT2DDS_FUNCTION_TIMING_LOG_PATH")
            .unwrap_or_else(|_| "function_timing.log".to_string());

        // Create file appender for tracing output
        let file_appender = tracing_appender::rolling::never(".", &log_path);
        let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

        // Leak the guard to ensure it lives for the entire program lifetime
        // This is necessary because the guard must outlive the subscriber
        std::mem::forget(_guard);

        // Create formatting layer with timing information
        let file_layer = fmt::layer()
            .with_writer(non_blocking)
            .with_span_events(FmtSpan::CLOSE) // Log when spans close (with duration)
            .with_target(true) // Include module path
            .with_thread_ids(true) // Include thread ID
            .with_thread_names(true) // Include thread name
            .with_line_number(true) // Include line number
            .with_ansi(false) // Disable ANSI colors for file output
            .boxed();

        // Create filter for tracing events
        // This allows fine-grained control over what gets traced
        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("trace")) // Default to trace level
            .add_directive("int2dds=trace".parse().unwrap()); // Always trace int2dds crate

        // Initialize the global subscriber
        // Use try_init() to gracefully handle the case where a subscriber already exists
        match tracing_subscriber::registry().with(filter).with(file_layer).try_init() {
            Ok(_) => {
                log::info!("Function timing measurement enabled: log file = {}", log_path);
            }
            Err(e) => {
                log::warn!(
                    "Failed to initialize tracing subscriber (appears to be already set up): {}. \
                     Function timing may not be logged.",
                    e
                );
            }
        }
    });
}

/// Check if function timing is enabled
///
/// # Returns
///
/// `true` if INT2DDS_FUNCTION_TIMING environment variable is set to "true"
pub fn is_timing_enabled() -> bool {
    std::env::var("INT2DDS_FUNCTION_TIMING").unwrap_or_else(|_| "false".to_string()).to_lowercase()
        == "true"
}

/// Get the configured timing log path
///
/// # Returns
///
/// The path from INT2DDS_FUNCTION_TIMING_LOG_PATH or default "function_timing.log"
pub fn get_timing_log_path() -> String {
    std::env::var("INT2DDS_FUNCTION_TIMING_LOG_PATH")
        .unwrap_or_else(|_| "function_timing.log".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timing_disabled_by_default() {
        std::env::remove_var("INT2DDS_FUNCTION_TIMING");
        assert!(!is_timing_enabled());
    }

    #[test]
    fn test_timing_enabled() {
        std::env::set_var("INT2DDS_FUNCTION_TIMING", "true");
        assert!(is_timing_enabled());
        std::env::remove_var("INT2DDS_FUNCTION_TIMING");
    }

    #[test]
    fn test_default_log_path() {
        std::env::remove_var("INT2DDS_FUNCTION_TIMING_LOG_PATH");
        assert_eq!(get_timing_log_path(), "function_timing.log");
    }

    #[test]
    fn test_custom_log_path() {
        std::env::set_var("INT2DDS_FUNCTION_TIMING_LOG_PATH", "custom_timing.log");
        assert_eq!(get_timing_log_path(), "custom_timing.log");
        std::env::remove_var("INT2DDS_FUNCTION_TIMING_LOG_PATH");
    }
}
