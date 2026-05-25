//! Advanced Host Controller Interface DMA buffer allocation.

use crate::kernel::memory::frame_allocator::BumpFrameAllocator;

const DMA_PAGE_SIZE: usize = 4096;

#[derive(Clone, Copy)]
pub(super) struct AhciDmaBuffers {
    pub(super) command_list: u64,
    pub(super) received_fis: u64,
    pub(super) command_table: u64,
    pub(super) data: u64,
}

pub(super) fn allocate(frame_allocator: &mut BumpFrameAllocator) -> Option<AhciDmaBuffers> {
    let command_list = frame_allocator.allocate_frame()?;
    let received_fis = frame_allocator.allocate_frame()?;
    let command_table = frame_allocator.allocate_frame()?;
    let data = frame_allocator.allocate_frame()?;

    zero_page(command_list);
    zero_page(received_fis);
    zero_page(command_table);
    zero_page(data);

    crate::log_debug!(
        "ahci",
        "DMA buffers: command_list={:#018x} received_fis={:#018x} command_table={:#018x} data={:#018x}",
        command_list,
        received_fis,
        command_table,
        data
    );

    Some(AhciDmaBuffers {
        command_list,
        received_fis,
        command_table,
        data,
    })
}

pub(super) fn split_address(address: u64) -> (u32, u32) {
    (
        u32::try_from(address & 0xffff_ffff).expect("low address bits must fit in u32"),
        u32::try_from(address >> 32).expect("high address bits must fit in u32"),
    )
}

fn zero_page(physical_address: u64) {
    let pointer = physical_address as *mut u8;
    // SAFETY: DMA buffers come from freshly allocated identity-mapped frames.
    unsafe {
        core::ptr::write_bytes(pointer, 0, DMA_PAGE_SIZE);
    }
}
