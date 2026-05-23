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
use spin::{LazyLock, Mutex};
use uart_16550::{backend::PioBackend, Config, Uart16550};

// SAFETY: COM1 is the standard serial port base address, and all access is
// synchronized through SERIAL1.
static SERIAL1: LazyLock<Mutex<Uart16550<PioBackend>>> = LazyLock::new(|| {
    let mut serial_port = {
        // SAFETY: COM1 is the standard port-mapped IO base address for the first
        // serial controller on PC-compatible hardware.
        unsafe { Uart16550::new_port(0x3F8) }.expect("COM1 base port must be valid")
    };
    serial_port
        .init(Config::default())
        .expect("failed to initialize COM1 serial port");
    Mutex::new(serial_port)
});

/// Initialize the COM1 serial port.
pub fn init() {
    LazyLock::force(&SERIAL1);
}

/// Print formatted arguments through COM1.
#[doc(hidden)]
pub fn print(args: fmt::Arguments) {
    use fmt::Write;
    use x86_64::instructions::interrupts;

    interrupts::without_interrupts(|| {
        SerialWriter(&mut SERIAL1.lock()).write_fmt(args).unwrap();
    });
}

struct SerialWriter<'a>(&'a mut Uart16550<PioBackend>);

impl fmt::Write for SerialWriter<'_> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        for byte in text.bytes() {
            match byte {
                b'\n' => self.0.send_bytes_exact(b"\r\n"),
                data => self.0.send_bytes_exact(&[data]),
            }
        }
        Ok(())
    }
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
