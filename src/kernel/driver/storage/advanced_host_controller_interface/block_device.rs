//! Advanced Host Controller Interface block device adapter.

use crate::kernel::driver::storage::block_device::BlockDevice;

use super::command;
use super::dma::AhciDmaBuffers;
use super::registers::HbaPort;

pub(super) struct AhciBlockDevice {
    port: *mut HbaPort,
    buffers: AhciDmaBuffers,
    port_index: usize,
}

impl AhciBlockDevice {
    pub(super) fn new(port: *mut HbaPort, buffers: AhciDmaBuffers, port_index: usize) -> Self {
        Self {
            port,
            buffers,
            port_index,
        }
    }
}

impl BlockDevice for AhciBlockDevice {
    fn read_logical_block(&mut self, logical_block_address: u64, data_address: u64) -> bool {
        if data_address != self.buffers.data {
            crate::log_error!(
                "ahci",
                "unexpected AHCI read buffer: requested={:#018x} owned={:#018x}",
                data_address,
                self.buffers.data
            );
            return false;
        }

        command::issue_read_sector(
            self.port,
            self.buffers,
            self.port_index,
            logical_block_address,
        )
    }
}
