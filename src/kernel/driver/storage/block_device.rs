//! Block-device abstraction for storage parsers.

use crate::kernel::memory::address::StorageDataAddress;

/// Number of bytes in one storage sector used by early block readers.
pub(super) const SECTOR_BYTES: usize = 512;

/// Error reported by a block-device operation.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(super) enum BlockDeviceError {
    /// The requested LBA range is outside the device or partition.
    OutOfRange,
    /// Address or byte-count arithmetic overflowed.
    Overflow,
    /// The caller passed a buffer not owned by the active DMA device.
    BufferMismatch,
    /// The transfer sector count is zero or larger than the device supports.
    InvalidTransferLength,
    /// The device stayed busy until the command timeout expired.
    DeviceBusyTimeout,
    /// A submitted command did not complete before the timeout expired.
    CommandTimeout,
    /// Hardware reported a task-file error for the submitted command.
    TaskFileError,
    /// The device does not support the requested operation.
    Unsupported,
}

/// Result type for block-device operations.
pub(super) type BlockDeviceResult<T> = Result<T, BlockDeviceError>;

/// Device capable of reading 512-byte sectors by logical block address.
pub(super) trait BlockDevice {
    /// Read one sector into the provided physical memory address.
    fn read_logical_block(
        &mut self,
        logical_block_address: u64,
        data_address: StorageDataAddress,
    ) -> BlockDeviceResult<()> {
        self.read_logical_blocks(logical_block_address, 1, data_address)
    }

    /// Read one or more contiguous sectors into the provided physical address.
    fn read_logical_blocks(
        &mut self,
        logical_block_address: u64,
        sector_count: u16,
        data_address: StorageDataAddress,
    ) -> BlockDeviceResult<()>;

    /// Write one or more contiguous sectors from the provided physical address.
    #[allow(dead_code)]
    fn write_logical_blocks(
        &mut self,
        _logical_block_address: u64,
        _sector_count: u16,
        _data_address: StorageDataAddress,
    ) -> BlockDeviceResult<()> {
        Err(BlockDeviceError::Unsupported)
    }
}
