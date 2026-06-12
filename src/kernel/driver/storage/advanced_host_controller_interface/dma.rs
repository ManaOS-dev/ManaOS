//! Advanced Host Controller Interface DMA buffer allocation.

use crate::kernel::memory::{
    address::DmaPhysicalAddress,
    frame_allocator::{BumpFrameAllocator, FrameRangeOwner},
};

const DMA_PAGE_SIZE: usize = 4096;
const DATA_BUFFER_PAGES: u64 = 16;
const DATA_BUFFER_BYTES: usize = 16 * DMA_PAGE_SIZE;

#[derive(Clone, Copy)]
pub(super) struct AhciDmaBuffers {
    pub(super) command_list: DmaPhysicalAddress,
    pub(super) received_fis: DmaPhysicalAddress,
    pub(super) command_table: DmaPhysicalAddress,
    pub(super) data: DmaPhysicalAddress,
    pub(super) data_bytes: usize,
}

pub(super) fn allocate(frame_allocator: &mut BumpFrameAllocator) -> Option<AhciDmaBuffers> {
    let command_list = frame_allocator.allocate_frame_for(FrameRangeOwner::AhciDma)?;
    let received_fis = frame_allocator.allocate_frame_for(FrameRangeOwner::AhciDma)?;
    let command_table = frame_allocator.allocate_frame_for(FrameRangeOwner::AhciDma)?;
    let data = frame_allocator.allocate_frames_for(DATA_BUFFER_PAGES, FrameRangeOwner::AhciDma)?;
    let command_list_address = command_list.as_dma_address();
    let received_fis_address = received_fis.as_dma_address();
    let command_table_address = command_table.as_dma_address();
    let data_address = data.start().as_dma_address();

    zero_page(command_list_address);
    zero_page(received_fis_address);
    zero_page(command_table_address);
    zero_range(data_address, DATA_BUFFER_BYTES);

    crate::log_debug!(
        "ahci",
        "DMA buffers: command_list={:#018x} received_fis={:#018x} command_table={:#018x} data={:#018x} data_bytes={}",
        command_list_address.as_u64(),
        received_fis_address.as_u64(),
        command_table_address.as_u64(),
        data_address.as_u64(),
        DATA_BUFFER_BYTES
    );
    crate::log_info!(
        "ahci",
        "DMA ownership: AHCI owns the data buffer only while a serialized command is in flight"
    );

    Some(AhciDmaBuffers {
        command_list: command_list_address,
        received_fis: received_fis_address,
        command_table: command_table_address,
        data: data_address,
        data_bytes: DATA_BUFFER_BYTES,
    })
}

pub(super) fn split_address(address: DmaPhysicalAddress) -> (u32, u32) {
    let address = address.as_u64();
    (
        u32::try_from(address & 0xffff_ffff).expect("low address bits must fit in u32"),
        u32::try_from(address >> 32).expect("high address bits must fit in u32"),
    )
}

fn zero_page(physical_address: DmaPhysicalAddress) {
    zero_range(physical_address, DMA_PAGE_SIZE);
}

fn zero_range(physical_address: DmaPhysicalAddress, byte_count: usize) {
    let pointer = physical_address.as_usize() as *mut u8;
    // SAFETY: DMA buffers come from freshly allocated identity-mapped frames.
    unsafe {
        core::ptr::write_bytes(pointer, 0, byte_count);
    }
}
