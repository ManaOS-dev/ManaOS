//! Display helpers for FAT32 diagnostics.

/// Display wrapper for escaped ASCII byte slices.
pub(super) struct EscapedAscii<'a>(pub(super) &'a [u8]);

impl core::fmt::Display for EscapedAscii<'_> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for byte in self.0 {
            match *byte {
                b'\r' => write!(formatter, "\\r")?,
                b'\n' => write!(formatter, "\\n")?,
                b'\t' => write!(formatter, "\\t")?,
                0x20..=0x7e => write!(formatter, "{}", char::from(*byte))?,
                _ => write!(formatter, "\\x{byte:02x}")?,
            }
        }
        Ok(())
    }
}
