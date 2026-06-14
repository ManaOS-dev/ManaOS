//! Structured kernel log output.

use core::fmt;
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};

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

/// One key/value field attached to a structured log message.
pub struct LogField<'a> {
    key: &'a str,
    value: fmt::Arguments<'a>,
}

impl<'a> LogField<'a> {
    /// Create one structured log field.
    pub const fn new(key: &'a str, value: fmt::Arguments<'a>) -> Self {
        Self { key, value }
    }
}

const MAX_LEVEL: LogLevel = if cfg!(feature = "log-trace") {
    LogLevel::Trace
} else if cfg!(feature = "log-debug") {
    LogLevel::Debug
} else {
    LogLevel::Info
};
const COMPONENT_LEVEL_WIDTH: usize = 18;
const DETAIL_INDENT: &str = "                                 ";
const FIELD_KEY_WIDTH: usize = 35;
const MICROS_PER_SECOND: u64 = 1_000_000;
const MICROS_PER_MILLI: u64 = 1_000;
const MIN_TSC_CLOCK_FREQUENCY: u64 = 1_000_000;
const CLOCK_SOURCE_BOOT: u8 = 0;
const CLOCK_SOURCE_TIMER: u8 = 1;
const CLOCK_SOURCE_TSC: u8 = 2;

static LAST_TIMESTAMP_MICROS: AtomicU64 = AtomicU64::new(0);
static CLOCK_SOURCE: AtomicU8 = AtomicU8::new(CLOCK_SOURCE_BOOT);
static TSC_UPGRADE_LOGGED: AtomicBool = AtomicBool::new(false);
static TSC_BASE_COUNTER: AtomicU64 = AtomicU64::new(0);
static TSC_BASE_MICROS: AtomicU64 = AtomicU64::new(0);
static TSC_FREQUENCY_HERTZ: AtomicU64 = AtomicU64::new(0);

/// Emit a structured kernel log line to the serial console.
pub fn log(level: LogLevel, category: &str, args: fmt::Arguments) {
    if level > MAX_LEVEL {
        return;
    }

    emit_log_line(timestamp_micros(), level, category, args);
}

/// Emit a structured kernel log entry with aligned key/value detail lines.
pub fn log_kv(level: LogLevel, category: &str, message: fmt::Arguments, fields: &[LogField]) {
    if level > MAX_LEVEL {
        return;
    }

    let timestamp = timestamp_micros();
    emit_log_line(timestamp, level, category, message);
    for field in fields {
        emit_field(field);
    }
}

/// Emit a boot log section heading.
pub fn section(title: &str) {
    crate::serial_println!("\n──────────────── {} ────────────────\n", title);
}

/// Record that the boot log clock can use a calibrated timestamp counter.
pub fn upgrade_clock_to_tsc(frequency_hertz: u64) {
    if frequency_hertz < MIN_TSC_CLOCK_FREQUENCY || TSC_UPGRADE_LOGGED.load(Ordering::Acquire) {
        return;
    }

    let previous_source = CLOCK_SOURCE.load(Ordering::Acquire);
    let base_micros = timestamp_micros();
    let base_counter = crate::kernel::profiler::read_tsc();
    if base_counter == 0 {
        return;
    }
    if TSC_UPGRADE_LOGGED.swap(true, Ordering::AcqRel) {
        return;
    }

    TSC_BASE_MICROS.store(base_micros, Ordering::Release);
    TSC_BASE_COUNTER.store(base_counter, Ordering::Release);
    TSC_FREQUENCY_HERTZ.store(frequency_hertz, Ordering::Release);
    CLOCK_SOURCE.store(CLOCK_SOURCE_TSC, Ordering::Release);

    let timestamp = timestamp_micros();
    let previous_source_label = clock_source_label(previous_source);
    let frequency_mhz = frequency_hertz / 1_000_000;
    emit_log_line(
        timestamp,
        LogLevel::Info,
        "time",
        format_args!("Log clock upgraded"),
    );
    emit_raw_field("from", format_args!("{previous_source_label}"));
    emit_raw_field("to", format_args!("TSC"));
    emit_raw_field("frequency", format_args!("{frequency_mhz} MHz"));
}

fn level_label(level: LogLevel) -> &'static str {
    match level {
        LogLevel::Error => "error",
        LogLevel::Warn => "warn",
        LogLevel::Info => "info",
        LogLevel::Debug => "debug",
        LogLevel::Trace => "trace",
    }
}

fn emit_log_line(timestamp_micros: u64, level: LogLevel, category: &str, args: fmt::Arguments) {
    let level_name = level_label(level);
    let whole_millis = timestamp_micros / MICROS_PER_MILLI;
    let fractional_micros = timestamp_micros % MICROS_PER_MILLI;
    let component_level_len = category
        .len()
        .saturating_add(1)
        .saturating_add(level_name.len());
    let component_padding = COMPONENT_LEVEL_WIDTH.saturating_sub(component_level_len);
    let padding = "";
    crate::serial_print!(
        "+{whole_millis:04}.{fractional_micros:03}ms  {category}.{level_name}{padding:component_padding$}  {args}\n"
    );
}

fn emit_field(field: &LogField) {
    emit_raw_field(field.key, field.value);
}

fn emit_raw_field(key: &str, value: fmt::Arguments) {
    let indent = DETAIL_INDENT;
    let key_width = FIELD_KEY_WIDTH;
    crate::serial_print!("{indent}{key:<key_width$} = {value}\n");
}

fn timestamp_micros() -> u64 {
    let (candidate, source) = candidate_timestamp_micros();
    if source != CLOCK_SOURCE_TSC && CLOCK_SOURCE.load(Ordering::Acquire) != CLOCK_SOURCE_TSC {
        CLOCK_SOURCE.store(source, Ordering::Release);
    }
    monotonic_timestamp(candidate)
}

fn candidate_timestamp_micros() -> (u64, u8) {
    if CLOCK_SOURCE.load(Ordering::Acquire) == CLOCK_SOURCE_TSC {
        let tsc_frequency = TSC_FREQUENCY_HERTZ.load(Ordering::Acquire);
        let base_counter = TSC_BASE_COUNTER.load(Ordering::Acquire);
        let base_micros = TSC_BASE_MICROS.load(Ordering::Acquire);
        let timestamp_counter = crate::kernel::profiler::read_tsc();
        if tsc_frequency >= MIN_TSC_CLOCK_FREQUENCY && base_counter > 0 && timestamp_counter > 0 {
            let elapsed_cycles = timestamp_counter.saturating_sub(base_counter);
            return (
                base_micros.saturating_add(timestamp_counter_delta_to_micros(
                    elapsed_cycles,
                    tsc_frequency,
                )),
                CLOCK_SOURCE_TSC,
            );
        }
    }

    let timer_ticks = crate::kernel::time::get_timer_ticks();
    if timer_ticks > 0 {
        return (
            timer_ticks.saturating_mul(MICROS_PER_SECOND) / crate::shared::TIMER_TICKS_PER_SECOND,
            CLOCK_SOURCE_TIMER,
        );
    }

    (0, CLOCK_SOURCE_BOOT)
}

fn timestamp_counter_delta_to_micros(delta_cycles: u64, frequency: u64) -> u64 {
    let whole_seconds = delta_cycles / frequency;
    let remaining_cycles = delta_cycles % frequency;
    whole_seconds
        .saturating_mul(MICROS_PER_SECOND)
        .saturating_add(remaining_cycles.saturating_mul(MICROS_PER_SECOND) / frequency)
}

fn monotonic_timestamp(candidate: u64) -> u64 {
    let mut previous = LAST_TIMESTAMP_MICROS.load(Ordering::Acquire);
    loop {
        let next = candidate.max(previous);
        match LAST_TIMESTAMP_MICROS.compare_exchange_weak(
            previous,
            next,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => return next,
            Err(observed) => previous = observed,
        }
    }
}

fn clock_source_label(source: u8) -> &'static str {
    match source {
        CLOCK_SOURCE_TIMER => "PIT",
        CLOCK_SOURCE_TSC => "TSC",
        _ => "boot",
    }
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
