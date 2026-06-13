//! Frame allocator diagnostics snapshots for console and boot smoke checks.

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use super::frame_allocator::{FrameAllocatorStatistics, PhysicalFrameAllocator};

static SNAPSHOT_READY: AtomicBool = AtomicBool::new(false);
static RESERVED_PAGES: AtomicU64 = AtomicU64::new(0);
static FREE_PAGES: AtomicU64 = AtomicU64::new(0);
static USED_PAGES: AtomicU64 = AtomicU64::new(0);
static OWNER_FREE_PAGES: AtomicU64 = AtomicU64::new(0);
static FIRMWARE_RESERVED_PAGES: AtomicU64 = AtomicU64::new(0);
static KERNEL_IMAGE_PAGES: AtomicU64 = AtomicU64::new(0);
static MMIO_PAGES: AtomicU64 = AtomicU64::new(0);
static GUARD_PAGE_PAGES: AtomicU64 = AtomicU64::new(0);
static UNKNOWN_USED_PAGES: AtomicU64 = AtomicU64::new(0);
static PAGE_TABLE_PAGES: AtomicU64 = AtomicU64::new(0);
static KERNEL_HEAP_PAGES: AtomicU64 = AtomicU64::new(0);
static KERNEL_STACK_PAGES: AtomicU64 = AtomicU64::new(0);
static FRAMEBUFFER_BACKBUFFER_PAGES: AtomicU64 = AtomicU64::new(0);
static AHCI_DMA_PAGES: AtomicU64 = AtomicU64::new(0);
static DYNAMIC_KERNEL_MAPPING_PAGES: AtomicU64 = AtomicU64::new(0);
static USER_STACK_PAGES: AtomicU64 = AtomicU64::new(0);
static USER_ELF_PAGES: AtomicU64 = AtomicU64::new(0);
static USER_HEAP_PAGES: AtomicU64 = AtomicU64::new(0);
static USER_MAPPING_PAGES: AtomicU64 = AtomicU64::new(0);

/// Physical frame allocator owner totals captured at a specific boot point.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FrameAllocatorOwnerDiagnostics {
    free: u64,
    firmware_reserved: u64,
    kernel_image: u64,
    mmio: u64,
    guard_page: u64,
    unknown_used: u64,
    page_table: u64,
    kernel_heap: u64,
    kernel_stack: u64,
    framebuffer_backbuffer: u64,
    ahci_dma: u64,
    dynamic_kernel_mapping: u64,
    user_stack: u64,
    user_elf: u64,
    user_heap: u64,
    user_mapping: u64,
}

impl FrameAllocatorOwnerDiagnostics {
    /// Return pages with no active frame owner.
    pub const fn free(self) -> u64 {
        self.free
    }

    /// Return firmware-reserved pages.
    pub const fn firmware_reserved(self) -> u64 {
        self.firmware_reserved
    }

    /// Return pages containing the loaded kernel image.
    pub const fn kernel_image(self) -> u64 {
        self.kernel_image
    }

    /// Return pages reserved for memory-mapped I/O.
    pub const fn mmio(self) -> u64 {
        self.mmio
    }

    /// Return pages reserved as explicit guard pages.
    pub const fn guard_page(self) -> u64 {
        self.guard_page
    }

    /// Return used pages whose owner is not yet classified.
    pub const fn unknown_used(self) -> u64 {
        self.unknown_used
    }

    /// Return page-table pages.
    pub const fn page_table(self) -> u64 {
        self.page_table
    }

    /// Return kernel heap pages.
    pub const fn kernel_heap(self) -> u64 {
        self.kernel_heap
    }

    /// Return guarded kernel stack pages.
    pub const fn kernel_stack(self) -> u64 {
        self.kernel_stack
    }

    /// Return framebuffer backbuffer pages.
    pub const fn framebuffer_backbuffer(self) -> u64 {
        self.framebuffer_backbuffer
    }

    /// Return AHCI DMA pages.
    pub const fn ahci_dma(self) -> u64 {
        self.ahci_dma
    }

    /// Return temporary dynamic kernel mapping pages.
    pub const fn dynamic_kernel_mapping(self) -> u64 {
        self.dynamic_kernel_mapping
    }

    /// Return user stack pages.
    pub const fn user_stack(self) -> u64 {
        self.user_stack
    }

    /// Return user ELF segment pages.
    pub const fn user_elf(self) -> u64 {
        self.user_elf
    }

    /// Return user heap pages.
    pub const fn user_heap(self) -> u64 {
        self.user_heap
    }

    /// Return private user mapping pages.
    pub const fn user_mapping(self) -> u64 {
        self.user_mapping
    }

    /// Return all currently user-owned physical pages.
    pub const fn user_pages(self) -> u64 {
        self.user_stack
            .saturating_add(self.user_elf)
            .saturating_add(self.user_heap)
            .saturating_add(self.user_mapping)
    }
}

/// Physical frame allocator totals captured at a specific boot point.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FrameAllocatorDiagnostics {
    reserved: u64,
    free: u64,
    used: u64,
    owners: FrameAllocatorOwnerDiagnostics,
}

impl FrameAllocatorDiagnostics {
    const fn new(
        statistics: FrameAllocatorStatistics,
        owners: FrameAllocatorOwnerDiagnostics,
    ) -> Self {
        Self {
            reserved: statistics.reserved,
            free: statistics.free,
            used: statistics.used,
            owners,
        }
    }

    /// Return tracked reserved pages.
    pub const fn reserved(self) -> u64 {
        self.reserved
    }

    /// Return tracked free pages.
    pub const fn free(self) -> u64 {
        self.free
    }

    /// Return tracked used pages.
    pub const fn used(self) -> u64 {
        self.used
    }

    /// Return all tracked pages.
    pub const fn total(self) -> u64 {
        self.reserved
            .saturating_add(self.free)
            .saturating_add(self.used)
    }

    /// Return captured owner totals.
    pub const fn owners(self) -> FrameAllocatorOwnerDiagnostics {
        self.owners
    }
}

/// Record the current physical frame allocator statistics for later diagnostics.
pub fn record_frame_allocator_snapshot(frame_allocator: &PhysicalFrameAllocator) {
    let statistics = frame_allocator.statistics();
    let owners = frame_allocator.owner_statistics();

    RESERVED_PAGES.store(statistics.reserved, Ordering::Relaxed);
    FREE_PAGES.store(statistics.free, Ordering::Relaxed);
    USED_PAGES.store(statistics.used, Ordering::Relaxed);
    OWNER_FREE_PAGES.store(owners.free, Ordering::Relaxed);
    FIRMWARE_RESERVED_PAGES.store(owners.firmware_reserved, Ordering::Relaxed);
    KERNEL_IMAGE_PAGES.store(owners.kernel_image, Ordering::Relaxed);
    MMIO_PAGES.store(owners.mmio, Ordering::Relaxed);
    GUARD_PAGE_PAGES.store(owners.guard_page, Ordering::Relaxed);
    UNKNOWN_USED_PAGES.store(owners.unknown_used, Ordering::Relaxed);
    PAGE_TABLE_PAGES.store(owners.page_table, Ordering::Relaxed);
    KERNEL_HEAP_PAGES.store(owners.kernel_heap, Ordering::Relaxed);
    KERNEL_STACK_PAGES.store(owners.kernel_stack, Ordering::Relaxed);
    FRAMEBUFFER_BACKBUFFER_PAGES.store(owners.framebuffer_backbuffer, Ordering::Relaxed);
    AHCI_DMA_PAGES.store(owners.ahci_dma, Ordering::Relaxed);
    DYNAMIC_KERNEL_MAPPING_PAGES.store(owners.dynamic_kernel_mapping, Ordering::Relaxed);
    USER_STACK_PAGES.store(owners.user_stack, Ordering::Relaxed);
    USER_ELF_PAGES.store(owners.user_elf, Ordering::Relaxed);
    USER_HEAP_PAGES.store(owners.user_heap, Ordering::Relaxed);
    USER_MAPPING_PAGES.store(owners.user_mapping, Ordering::Relaxed);
    SNAPSHOT_READY.store(true, Ordering::Release);
}

/// Return the latest recorded physical frame allocator diagnostics.
pub fn get_frame_allocator_diagnostics() -> Option<FrameAllocatorDiagnostics> {
    if !SNAPSHOT_READY.load(Ordering::Acquire) {
        return None;
    }

    Some(FrameAllocatorDiagnostics::new(
        FrameAllocatorStatistics {
            reserved: RESERVED_PAGES.load(Ordering::Relaxed),
            free: FREE_PAGES.load(Ordering::Relaxed),
            used: USED_PAGES.load(Ordering::Relaxed),
        },
        FrameAllocatorOwnerDiagnostics {
            free: OWNER_FREE_PAGES.load(Ordering::Relaxed),
            firmware_reserved: FIRMWARE_RESERVED_PAGES.load(Ordering::Relaxed),
            kernel_image: KERNEL_IMAGE_PAGES.load(Ordering::Relaxed),
            mmio: MMIO_PAGES.load(Ordering::Relaxed),
            guard_page: GUARD_PAGE_PAGES.load(Ordering::Relaxed),
            unknown_used: UNKNOWN_USED_PAGES.load(Ordering::Relaxed),
            page_table: PAGE_TABLE_PAGES.load(Ordering::Relaxed),
            kernel_heap: KERNEL_HEAP_PAGES.load(Ordering::Relaxed),
            kernel_stack: KERNEL_STACK_PAGES.load(Ordering::Relaxed),
            framebuffer_backbuffer: FRAMEBUFFER_BACKBUFFER_PAGES.load(Ordering::Relaxed),
            ahci_dma: AHCI_DMA_PAGES.load(Ordering::Relaxed),
            dynamic_kernel_mapping: DYNAMIC_KERNEL_MAPPING_PAGES.load(Ordering::Relaxed),
            user_stack: USER_STACK_PAGES.load(Ordering::Relaxed),
            user_elf: USER_ELF_PAGES.load(Ordering::Relaxed),
            user_heap: USER_HEAP_PAGES.load(Ordering::Relaxed),
            user_mapping: USER_MAPPING_PAGES.load(Ordering::Relaxed),
        },
    ))
}
