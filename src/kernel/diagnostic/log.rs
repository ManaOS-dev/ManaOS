//! Structured kernel log output.

use core::fmt;

/// Kernel log severity level.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    /// Fatal or unrecoverable error.
    Error = 0,
    /// Recoverable issue or suspicious state.
    Warn = 1,
    /// Important boot or runtime milestone.
    Info = 2,
    /// Detailed state useful during driver and subsystem debugging.
    Debug = 3,
    /// Very verbose per-step diagnostics.
    Trace = 4,
}

const MAX_LEVEL: LogLevel = if cfg!(feature = "log-trace") {
    LogLevel::Trace
} else if cfg!(feature = "log-debug") {
    LogLevel::Debug
} else {
    LogLevel::Info
};

/// Emit a structured kernel log line to the serial console.
pub fn log(level: LogLevel, category: &str, args: fmt::Arguments) {
    if level > MAX_LEVEL {
        return;
    }

    let timestamp_millis = timestamp_millis();
    crate::serial_println!(
        "[{}:{:<8}] {:02}:{:02}.{:03} - {}",
        level_label(level),
        category,
        (timestamp_millis / 60_000) % 100,
        (timestamp_millis / 1_000) % 60,
        timestamp_millis % 1_000,
        args
    );
}

fn level_label(level: LogLevel) -> &'static str {
    match level {
        LogLevel::Error => "ERROR",
        LogLevel::Warn => "WARN ",
        LogLevel::Info => "INFO ",
        LogLevel::Debug => "DEBUG",
        LogLevel::Trace => "TRACE",
    }
}

fn timestamp_millis() -> u64 {
    let tsc_frequency = crate::kernel::profiler::get_tsc_frequency();
    if tsc_frequency >= 1_000 {
        return crate::kernel::profiler::read_tsc() / (tsc_frequency / 1_000);
    }

    crate::kernel::time::get_timer_ticks()
}

/// Emit an ERROR level kernel log line.
#[macro_export]
macro_rules! log_error {
    ($category:expr, $($arg:tt)*) => {
        $crate::kernel::diagnostic::log::log(
            $crate::kernel::diagnostic::log::LogLevel::Error,
            $category,
            format_args!($($arg)*),
        )
    };
}

/// Emit a WARN level kernel log line.
#[macro_export]
macro_rules! log_warn {
    ($category:expr, $($arg:tt)*) => {
        $crate::kernel::diagnostic::log::log(
            $crate::kernel::diagnostic::log::LogLevel::Warn,
            $category,
            format_args!($($arg)*),
        )
    };
}

/// Emit an INFO level kernel log line.
#[macro_export]
macro_rules! log_info {
    ($category:expr, $($arg:tt)*) => {
        $crate::kernel::diagnostic::log::log(
            $crate::kernel::diagnostic::log::LogLevel::Info,
            $category,
            format_args!($($arg)*),
        )
    };
}

/// Emit a DEBUG level kernel log line when `log-debug` is enabled.
#[macro_export]
macro_rules! log_debug {
    ($category:expr, $($arg:tt)*) => {
        $crate::kernel::diagnostic::log::log(
            $crate::kernel::diagnostic::log::LogLevel::Debug,
            $category,
            format_args!($($arg)*),
        )
    };
}

/// Emit a TRACE level kernel log line when `log-trace` is enabled.
#[macro_export]
macro_rules! log_trace {
    ($category:expr, $($arg:tt)*) => {
        $crate::kernel::diagnostic::log::log(
            $crate::kernel::diagnostic::log::LogLevel::Trace,
            $category,
            format_args!($($arg)*),
        )
    };
}
