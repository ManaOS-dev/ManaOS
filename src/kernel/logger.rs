/// UEFI `ConOut` based logger, usable before `ExitBootServices`.
/// Automatically switches to serial output after `ExitBootServices`.
use core::fmt::Write;
use core::sync::atomic::{AtomicBool, Ordering};
use log::{LevelFilter, Log, Metadata, Record};

struct UefiLogger;

impl Log for UefiLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        if BOOT_LOGGING_ENABLED.load(Ordering::Acquire) {
            uefi::system::with_stdout(|stdout| {
                let _ = writeln!(stdout, "[{:5}] {}", record.level(), record.args());
            });
        } else {
            crate::serial_println!("[{:5}] {}", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}

static LOGGER: UefiLogger = UefiLogger;

static BOOT_LOGGING_ENABLED: AtomicBool = AtomicBool::new(false);

/// Initialize the logger. Call at the very beginning of main.
pub fn init() {
    BOOT_LOGGING_ENABLED.store(true, Ordering::Release);
    log::set_logger(&LOGGER).unwrap();
    log::set_max_level(LevelFilter::Info);
}

/// Must be called before `ExitBootServices` to invalidate the pointer.
pub fn disable() {
    BOOT_LOGGING_ENABLED.store(false, Ordering::Release);
}
