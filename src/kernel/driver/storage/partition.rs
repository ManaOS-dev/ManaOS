//! Partition-relative block device adapter.

use super::block_device::BlockDevice;

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
    fn read_logical_block(&mut self, logical_block_address: u64, data_address: u64) -> bool {
        if logical_block_address >= self.sector_count {
            crate::log_warn!(
                "storage",
                "partition LBA out of range: lba={} sectors={}",
                logical_block_address,
                self.sector_count
            );
            return false;
        }

        let Some(disk_lba) = self.first_lba.checked_add(logical_block_address) else {
            crate::log_error!(
                "storage",
                "partition LBA overflow: first_lba={} lba={}",
                self.first_lba,
                logical_block_address
            );
            return false;
        };

        self.inner.read_logical_block(disk_lba, data_address)
    }
}
