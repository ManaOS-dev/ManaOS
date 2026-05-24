//! Block-device abstraction for storage parsers.

/// Number of bytes in one storage sector used by early block readers.
pub(super) const SECTOR_BYTES: usize = 512;

/// Device capable of reading 512-byte sectors by logical block address.
pub(super) trait BlockDevice {
    /// Read one sector into the provided physical memory address.
    fn read_logical_block(&mut self, logical_block_address: u64, data_address: u64) -> bool;
}
