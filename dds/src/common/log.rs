//! Logging configuration types and utilities.
//!
//! This module provides logging configuration types (`LogType`, `LogLevel`) and functions
//! for setting up the logging infrastructure used throughout int2dds.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogType {
    Console,
    File,
    All,
    #[default]
    None,
}

impl std::fmt::Display for LogType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogType::Console => write!(f, "console"),
            LogType::File => write!(f, "file"),
            LogType::All => write!(f, "all"),
            LogType::None => write!(f, "none"),
        }
    }
}

impl std::str::FromStr for LogType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "console" => Ok(LogType::Console),
            "file" => Ok(LogType::File),
            "all" => Ok(LogType::All),
            "none" => Ok(LogType::None),
            _ => Err(format!(
                "Invalid transport type: {}. Valid options are 'console' or 'file' or 'all' or 'none'",
                s
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogLevel {
    Trace,
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Trace => write!(f, "trace"),
            LogLevel::Debug => write!(f, "debug"),
            LogLevel::Info => write!(f, "info"),
            LogLevel::Warn => write!(f, "warn"),
            LogLevel::Error => write!(f, "error"),
        }
    }
}

impl std::str::FromStr for LogLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "trace" => Ok(LogLevel::Trace),
            "debug" => Ok(LogLevel::Debug),
            "info" => Ok(LogLevel::Info),
            "warn" => Ok(LogLevel::Warn),
            "error" => Ok(LogLevel::Error),
            _ => Err(format!(
                "Invalid log level: {}. Valid options are 'trace', 'debug', 'info', 'warn', or 'error'",
                s
            )),
        }
    }
}

impl LogLevel {
    pub fn as_level_filter(&self) -> log::LevelFilter {
        match self {
            LogLevel::Trace => log::LevelFilter::Trace,
            LogLevel::Debug => log::LevelFilter::Debug,
            LogLevel::Info => log::LevelFilter::Info,
            LogLevel::Warn => log::LevelFilter::Warn,
            LogLevel::Error => log::LevelFilter::Error,
        }
    }
}

pub fn get_logger_type() -> LogType {
    std::env::var("INT2DDS_LOG_TYPE").ok().and_then(|val| val.parse().ok()).unwrap_or_default()
}

pub fn get_console_logger_level() -> LogLevel {
    std::env::var("INT2DDS_CONSOLE_LOG_LEVEL")
        .ok()
        .and_then(|val| val.parse().ok())
        .unwrap_or_default()
}

pub fn get_file_logger_level() -> LogLevel {
    std::env::var("INT2DDS_FILE_LOG_LEVEL")
        .ok()
        .and_then(|val| val.parse().ok())
        .unwrap_or_default()
}

pub fn setting_log() {
    if get_logger_type() == LogType::None {
        return;
    }

    let log_file_name = format!("log_{}.log", chrono::Local::now().format("%Y%m%d_%H%M%S"));

    let base =
        fern::Dispatch::new().level(log::LevelFilter::Trace).format(|out, message, record| {
            out.finish(format_args!(
                "[{}][{}][{}][{}] {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S.%f"),
                record.level(),
                std::thread::current().name().unwrap_or("unknown"),
                record.target(),
                message
            ))
        });

    let to_stdout = fern::Dispatch::new()
        .level(get_console_logger_level().as_level_filter()) // Apply console log level
        .chain(std::io::stdout());

    let configured = match get_logger_type() {
        LogType::Console => base.chain(to_stdout),
        LogType::File => {
            let to_run_file = fern::Dispatch::new()
                .level(get_file_logger_level().as_level_filter())
                .chain(fern::log_file(&log_file_name).unwrap());
            base.chain(to_run_file)
        }
        LogType::All => {
            let to_run_file = fern::Dispatch::new()
                .level(get_file_logger_level().as_level_filter())
                .chain(fern::log_file(&log_file_name).unwrap());
            base.chain(to_stdout).chain(to_run_file)
        }
        LogType::None => base,
    };

    if let Err(e) = configured.apply() {
        eprintln!("Logger already initialized: {}", e);
    }
}
