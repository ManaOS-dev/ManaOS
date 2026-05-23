/// UEFI `ConOut` based logger, usable before `ExitBootServices`.
/// Automatically switches to serial output after `ExitBootServices`.
use core::fmt::Write;
use log::{LevelFilter, Log, Metadata, Record};
use uefi::table::{Boot, SystemTable};

struct UefiLogger;

impl Log for UefiLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        // Direct write via global SystemTable pointer
        unsafe {
            if let Some(st) = SYSTEM_TABLE.as_mut() {
                let _ = writeln!(st.stdout(), "[{:5}] {}", record.level(), record.args());
            } else {
                crate::serial_println!("[{:5}] {}", record.level(), record.args());
            }
        }
    }

    fn flush(&self) {}
}

static LOGGER: UefiLogger = UefiLogger;

/// Global `SystemTable` pointer (Valid only during Boot Phase)
static mut SYSTEM_TABLE: *mut SystemTable<Boot> = core::ptr::null_mut();

/// Initialize the logger. Call at the very beginning of main.
pub fn init(st: &mut SystemTable<Boot>) {
    unsafe {
        SYSTEM_TABLE = core::ptr::from_mut::<SystemTable<Boot>>(st);
    }
    log::set_logger(&LOGGER).unwrap();
    log::set_max_level(LevelFilter::Info);
}

/// Must be called before `ExitBootServices` to invalidate the pointer.
pub fn disable() {
    unsafe {
        SYSTEM_TABLE = core::ptr::null_mut();
    }
}
