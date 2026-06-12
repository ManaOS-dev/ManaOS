//! Partition-relative block device adapter.

use super::block_device::{BlockDevice, BlockDeviceError, BlockDeviceResult};
use crate::kernel::memory::address::StorageDataAddress;

/// Block-device view that translates partition-relative LBAs to disk LBAs.
pub(super) struct PartitionBlockDevice<'a, T: BlockDevice> {
    inner: &'a mut T,
    first_lba: u64,
    sector_count: u64,
}

impl<'a, T: BlockDevice> PartitionBlockDevice<'a, T> {
    /// Create a partition-relative block-device view.
    pub(super) fn new(inner: &'a mut T, first_lba: u64, last_lba: u64) -> Self {
        let sector_count = last_lba
            .checked_sub(first_lba)
            .and_then(|value| value.checked_add(1))
            .unwrap_or(0);

        Self {
            inner,
            first_lba,
            sector_count,
        }
    }
}

impl<T: BlockDevice> BlockDevice for PartitionBlockDevice<'_, T> {
    fn read_logical_blocks(
        &mut self,
        logical_block_address: u64,
        sector_count: u16,
        data_address: StorageDataAddress,
    ) -> BlockDeviceResult<()> {
        if sector_count == 0 {
            return Err(BlockDeviceError::InvalidTransferLength);
        }

        let last_logical_block_address = logical_block_address
            .checked_add(u64::from(sector_count) - 1)
            .ok_or(BlockDeviceError::Overflow)?;
        if last_logical_block_address >= self.sector_count {
            crate::log_warn!(
                "storage",
                "partition LBA out of range: lba={} sector_count={} sectors={}",
                logical_block_address,
                sector_count,
                self.sector_count
            );
            return Err(BlockDeviceError::OutOfRange);
        }

        let disk_lba = self
            .first_lba
            .checked_add(logical_block_address)
            .ok_or(BlockDeviceError::Overflow)?;

        self.inner
            .read_logical_blocks(disk_lba, sector_count, data_address)
    }

    fn write_logical_blocks(
        &mut self,
        logical_block_address: u64,
        sector_count: u16,
        data_address: StorageDataAddress,
    ) -> BlockDeviceResult<()> {
        if sector_count == 0 {
            return Err(BlockDeviceError::InvalidTransferLength);
        }

        let last_logical_block_address = logical_block_address
            .checked_add(u64::from(sector_count) - 1)
            .ok_or(BlockDeviceError::Overflow)?;
        if last_logical_block_address >= self.sector_count {
            crate::log_warn!(
                "storage",
                "partition write LBA out of range: lba={} sector_count={} sectors={}",
                logical_block_address,
                sector_count,
                self.sector_count
            );
            return Err(BlockDeviceError::OutOfRange);
        }

        let disk_lba = self
            .first_lba
            .checked_add(logical_block_address)
            .ok_or(BlockDeviceError::Overflow)?;

        self.inner
            .write_logical_blocks(disk_lba, sector_count, data_address)
    }
}
