//! Advanced Host Controller Interface block device adapter.

use crate::kernel::driver::storage::block_device::{
    BlockDevice, BlockDeviceError, BlockDeviceResult, SECTOR_BYTES,
};

use super::command::{self, AhciTransferDirection};
use super::completion::CompletionMode;
use super::dma::AhciDmaBuffers;
use super::registers::HbaPort;

/// Persistent Advanced Host Controller Interface block-device service.
pub(in crate::kernel::driver::storage) struct AhciBlockDevice {
    port: *mut HbaPort,
    buffers: AhciDmaBuffers,
    port_index: usize,
    completion_mode: CompletionMode,
}

// SAFETY: Access to the raw port pointer and DMA buffers is serialized by the
// storage service mutex before commands are issued.
unsafe impl Send for AhciBlockDevice {}

impl AhciBlockDevice {
    /// Create a persistent AHCI block device for one SATA port.
    pub(super) fn new(
        port: *mut HbaPort,
        buffers: AhciDmaBuffers,
        port_index: usize,
        completion_mode: CompletionMode,
    ) -> Self {
        Self {
            port,
            buffers,
            port_index,
            completion_mode,
        }
    }

    /// Return the largest transfer accepted by the DMA bounce buffer.
    pub(super) fn maximum_transfer_sectors(&self) -> u16 {
        u16::try_from(self.buffers.data_bytes / SECTOR_BYTES)
            .expect("AHCI DMA transfer sector capacity must fit in u16")
    }

    /// Return the physical address of the persistent DMA data buffer.
    pub(super) fn data_address(&self) -> u64 {
        self.buffers.data.as_u64()
    }

    /// Return the HBA port index served by this block device.
    pub(super) fn port_index(&self) -> usize {
        self.port_index
    }

    fn validate_data_buffer(&self, sector_count: u16, data_address: u64) -> BlockDeviceResult<()> {
        if data_address != self.buffers.data.as_u64() {
            crate::log_error!(
                "ahci",
                "unexpected AHCI transfer buffer: requested={:#018x} owned={:#018x}",
                data_address,
                self.buffers.data.as_u64()
            );
            return Err(BlockDeviceError::BufferMismatch);
        }

        if sector_count == 0 || sector_count > self.maximum_transfer_sectors() {
            crate::log_error!(
                "ahci",
                "invalid AHCI transfer length: sector_count={} max={}",
                sector_count,
                self.maximum_transfer_sectors()
            );
            return Err(BlockDeviceError::InvalidTransferLength);
        }

        Ok(())
    }
}

impl BlockDevice for AhciBlockDevice {
    fn read_logical_blocks(
        &mut self,
        logical_block_address: u64,
        sector_count: u16,
        data_address: u64,
    ) -> BlockDeviceResult<()> {
        self.validate_data_buffer(sector_count, data_address)?;

        command::issue_dma_transfer(
            self.port,
            self.buffers,
            self.port_index,
            logical_block_address,
            sector_count,
            AhciTransferDirection::Read,
            self.completion_mode,
        )
    }

    fn write_logical_blocks(
        &mut self,
        logical_block_address: u64,
        sector_count: u16,
        data_address: u64,
    ) -> BlockDeviceResult<()> {
        self.validate_data_buffer(sector_count, data_address)?;

        command::issue_dma_transfer(
            self.port,
            self.buffers,
            self.port_index,
            logical_block_address,
            sector_count,
            AhciTransferDirection::Write,
            self.completion_mode,
        )
    }
}
