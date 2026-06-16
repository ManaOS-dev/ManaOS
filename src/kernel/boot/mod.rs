//! # `kernel::boot`
//!
//! ## Owns
//! - Kernel memory initialization after boot services exit
//! - Framebuffer graphics driver initialization
//!
//! ## Does NOT own
//! - Architecture-specific CPU and interrupt setup (-> `arch`)
//! - Scheduler setup (-> `kernel::task`)
//! - Main loop execution (-> `kernel::runtime`)
//!
//! ## Public API
//! - [`initialize`] - Initialize memory and display subsystems

use crate::kernel::driver::display::font::FontAssets;
use crate::kernel::driver::display::framebuffer::{self, FrameBufferInfo};
use crate::kernel::driver::display::renderer;
use crate::kernel::memory::address::{
    FrameCount, FramebufferPhysicalRange, KernelVirtualAddress, PhysAddr,
};
use crate::kernel::memory::frame_allocator::FrameRangeOwner;
use crate::kernel::memory::frame_allocator::PhysicalFrameAllocator;
use crate::kernel::memory::heap;
use crate::kernel::memory::paging;
use uefi::mem::memory_map::MemoryDescriptor;

/// Initialize kernel memory and framebuffer-backed display subsystems.
pub fn initialize<'a>(
    frame_allocator: &mut PhysicalFrameAllocator,
    mmap_entries: impl Iterator<Item = &'a MemoryDescriptor>,
    framebuffer_info: FrameBufferInfo,
    fonts: FontAssets,
    backbuffer_address: KernelVirtualAddress,
) {
    let framebuffer_size = framebuffer_info
        .stride
        .checked_mul(framebuffer_info.vertical_resolution)
        .and_then(|pixels| pixels.checked_mul(4))
        .and_then(|bytes| u64::try_from(bytes).ok())
        .expect("framebuffer byte size must fit in u64");
    let framebuffer_range = FramebufferPhysicalRange::new(
        PhysAddr::new(framebuffer_info.base_ptr as u64),
        framebuffer_size,
    )
    .expect("framebuffer range must be non-empty");

    // SAFETY: The frame allocator owns conventional memory from the boot memory
    // map, and the framebuffer range comes from the active UEFI graphics mode.
    unsafe {
        paging::init(frame_allocator, mmap_entries, framebuffer_range);
    }
    crate::log_info!("paging", "Page table switched.");

    let heap_frame_count =
        FrameCount::new(heap::HEAP_PAGES).expect("kernel heap frame count must be valid");
    let heap_range = frame_allocator
        .allocate_frames_for(heap_frame_count, FrameRangeOwner::KernelHeap)
        .expect("OOM: failed to allocate pages for kernel heap");
    // SAFETY: heap_range was allocated from the frame allocator and is
    // exclusively reserved for the kernel heap.
    unsafe {
        heap::init(heap_range);
    }
    crate::log_info!(
        "heap",
        "Initialized at {:#010x}, size: {} MB",
        heap_range.start().as_u64(),
        heap::HEAP_SIZE / (1024 * 1024)
    );

    framebuffer::init_global_graphics(framebuffer_info, fonts, backbuffer_address.as_mut_ptr());
    renderer::draw_boot_screen();
}
