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
use crate::kernel::memory::frame_allocator::BumpFrameAllocator;
use crate::kernel::memory::heap;
use crate::kernel::memory::paging;
use uefi::mem::memory_map::MemoryDescriptor;

/// Initialize kernel memory and framebuffer-backed display subsystems.
pub fn initialize<'a>(
    frame_allocator: &mut BumpFrameAllocator,
    mmap_entries: impl Iterator<Item = &'a MemoryDescriptor>,
    framebuffer_info: FrameBufferInfo,
    fonts: FontAssets,
    backbuffer_ptr: *mut u8,
) {
    let framebuffer_base = framebuffer_info.base_ptr as u64;
    let framebuffer_size =
        (framebuffer_info.stride * framebuffer_info.vertical_resolution * 4) as u64;

    // SAFETY: The frame allocator owns conventional memory from the boot memory
    // map, and the framebuffer range comes from the active UEFI graphics mode.
    unsafe {
        paging::init(
            frame_allocator,
            mmap_entries,
            framebuffer_base,
            framebuffer_size,
        );
    }
    crate::log_info!("paging", "Page table switched.");

    let heap_start_raw = frame_allocator
        .allocate_frames(heap::HEAP_PAGES)
        .expect("OOM: failed to allocate pages for kernel heap");
    let heap_start =
        usize::try_from(heap_start_raw).expect("failed to convert kernel heap address to usize");
    // SAFETY: heap_start was allocated from the frame allocator and is
    // exclusively reserved for the kernel heap.
    unsafe {
        heap::init(heap_start);
    }
    crate::log_info!(
        "heap",
        "Initialized at {:#010x}, size: {} MB",
        heap_start,
        heap::HEAP_SIZE / (1024 * 1024)
    );

    framebuffer::init_global_graphics(framebuffer_info, fonts, backbuffer_ptr);
    renderer::draw_boot_screen();
}
