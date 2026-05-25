//! Display helpers for GUID partition table diagnostics.

use core::fmt;

pub(super) struct GuidPartitionTableGuid<'a>(pub(super) &'a [u8]);

impl fmt::Display for GuidPartitionTableGuid<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let bytes = self.0;
        write!(
            formatter,
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-",
            bytes[3],
            bytes[2],
            bytes[1],
            bytes[0],
            bytes[5],
            bytes[4],
            bytes[7],
            bytes[6],
            bytes[8],
            bytes[9]
        )?;
        for byte in &bytes[10..16] {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}
