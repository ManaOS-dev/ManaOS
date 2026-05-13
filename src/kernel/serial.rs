//! # `kernel::serial`
//!
//! ## Owns
//! - COM1 serial output after boot services exit
//!
//! ## Does NOT own
//! - UEFI boot phase logging (-> `kernel::logger`)
//!
//! ## Public API
//! - [`init`] - Initialize COM1
//! - [`print`] - Internal formatting backend for serial macros

use core::fmt;
use spin::Mutex;
use uart_16550::SerialPort;

// SAFETY: COM1 is the standard serial port base address, and all access is
// synchronized through SERIAL1.
static SERIAL1: Mutex<SerialPort> = unsafe { Mutex::new(SerialPort::new(0x3F8)) };

/// Initialize the COM1 serial port.
pub fn init() {
    SERIAL1.lock().init();
}

/// Print formatted arguments through COM1.
#[doc(hidden)]
pub fn print(args: fmt::Arguments) {
    use fmt::Write;
    use x86_64::instructions::interrupts;

    interrupts::without_interrupts(|| {
        SERIAL1.lock().write_fmt(args).unwrap();
    });
}

/// Print formatted text to the serial port.
#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => ($crate::kernel::serial::print(format_args!($($arg)*)));
}

/// Print formatted text to the serial port with a trailing newline.
#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial_print!("\n"));
    ($fmt:expr) => ($crate::serial_print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => ($crate::serial_print!(concat!($fmt, "\n"), $($arg)*));
}
