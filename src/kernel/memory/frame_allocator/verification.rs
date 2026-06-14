//! Frame allocator self-check routines.

use super::{
    FrameAllocatorOwnerStatistics, FrameAllocatorStatistics, FrameRangeOwner,
    PhysicalFrameAllocator, FRAME_SIZE,
};
use crate::kernel::memory::address::PhysAddr;

/// Verify the frame-zero skip behavior for multi-frame allocations.
#[allow(dead_code)]
pub fn verify_zero_address_skip_for_multi_frame_allocations() -> bool {
    let mut frame_allocator = PhysicalFrameAllocator::new();
    frame_allocator.add_region(PhysAddr::new(0), 3);

    frame_allocator
        .allocate_frames(2)
        .map(|range| range.start().as_u64())
        == Some(FRAME_SIZE)
}

/// Verify reserved, used, and free frame range tracking.
#[allow(dead_code)]
pub fn verify_reserved_used_and_free_range_tracking() -> bool {
    let mut frame_allocator = PhysicalFrameAllocator::new();
    frame_allocator.reserve_region(PhysAddr::new(0), 1);
    frame_allocator.add_region(PhysAddr::new(FRAME_SIZE), 4);

    if frame_allocator.allocate_frames(2).is_none() {
        return false;
    }

    frame_allocator.statistics()
        == FrameAllocatorStatistics {
            reserved: 1,
            free: 2,
            used: 2,
        }
}

/// Verify that allocations never return the same physical frame twice.
#[allow(dead_code)]
pub fn verify_duplicate_allocation_rejection() -> bool {
    let mut frame_allocator = PhysicalFrameAllocator::new();
    frame_allocator.add_region(PhysAddr::new(0), 4);

    let Some(first_frame) = frame_allocator.allocate_frame() else {
        return false;
    };
    let Some(second_frame) = frame_allocator.allocate_frame() else {
        return false;
    };

    first_frame != second_frame
        && frame_allocator.statistics()
            == FrameAllocatorStatistics {
                reserved: 0,
                free: 1,
                used: 2,
            }
}

/// Verify that contiguous allocations do not cross registered region gaps.
#[allow(dead_code)]
pub fn verify_contiguous_allocation_boundaries() -> bool {
    let mut frame_allocator = PhysicalFrameAllocator::new();
    frame_allocator.add_region(PhysAddr::new(FRAME_SIZE), 1);
    frame_allocator.add_region(PhysAddr::new(3 * FRAME_SIZE), 2);

    frame_allocator
        .allocate_frames(2)
        .map(|range| range.start().as_u64())
        == Some(3 * FRAME_SIZE)
}

/// Verify that reserved ranges inside a free region are not allocated.
#[allow(dead_code)]
pub fn verify_reserved_range_exclusion() -> bool {
    let mut frame_allocator = PhysicalFrameAllocator::new();
    frame_allocator.add_region(PhysAddr::new(FRAME_SIZE), 4);
    frame_allocator.reserve_region(PhysAddr::new(2 * FRAME_SIZE), 1);

    let Some(first_frame) = frame_allocator.allocate_frame() else {
        return false;
    };
    let Some(second_frame) = frame_allocator.allocate_frame() else {
        return false;
    };

    first_frame.as_u64() == FRAME_SIZE
        && second_frame.as_u64() == 3 * FRAME_SIZE
        && frame_allocator.statistics()
            == FrameAllocatorStatistics {
                reserved: 1,
                free: 1,
                used: 2,
            }
}

/// Verify that used frame ranges record their explicit owners.
#[allow(dead_code)]
pub fn verify_owner_tracking() -> bool {
    let mut frame_allocator = PhysicalFrameAllocator::new();
    frame_allocator.add_region(PhysAddr::new(FRAME_SIZE), 8);

    if frame_allocator
        .allocate_frames_for(2, FrameRangeOwner::KernelHeap)
        .is_none()
    {
        return false;
    }
    if frame_allocator
        .allocate_frames_for(3, FrameRangeOwner::UserElf)
        .is_none()
    {
        return false;
    }

    frame_allocator.pages_owned_by(FrameRangeOwner::KernelHeap) == 2
        && frame_allocator.pages_owned_by(FrameRangeOwner::UserElf) == 3
        && frame_allocator.statistics()
            == FrameAllocatorStatistics {
                reserved: 0,
                free: 3,
                used: 5,
            }
}

/// Verify that released physical frames are reused and owner checked.
#[allow(dead_code)]
pub fn verify_released_frame_reuse() -> bool {
    let mut frame_allocator = PhysicalFrameAllocator::new();
    frame_allocator.add_region(PhysAddr::new(FRAME_SIZE), 3);

    let Some(dynamic_range) =
        frame_allocator.allocate_frames_for(2, FrameRangeOwner::DynamicKernelMapping)
    else {
        return false;
    };
    if frame_allocator
        .allocate_frame_for(FrameRangeOwner::UserStack)
        .is_none()
    {
        return false;
    }

    let wrong_owner_rejected =
        !frame_allocator.free_frames_for(dynamic_range, FrameRangeOwner::KernelStack);
    let released =
        frame_allocator.free_frames_for(dynamic_range, FrameRangeOwner::DynamicKernelMapping);
    let double_free_rejected =
        !frame_allocator.free_frames_for(dynamic_range, FrameRangeOwner::DynamicKernelMapping);
    let Some(reused_range) = frame_allocator.allocate_frames_for(2, FrameRangeOwner::KernelStack)
    else {
        return false;
    };

    wrong_owner_rejected
        && released
        && double_free_rejected
        && reused_range == dynamic_range
        && frame_allocator.pages_owned_by(FrameRangeOwner::DynamicKernelMapping) == 0
        && frame_allocator.pages_owned_by(FrameRangeOwner::KernelStack) == 2
        && frame_allocator.statistics()
            == FrameAllocatorStatistics {
                reserved: 0,
                free: 0,
                used: 3,
            }
}

/// Verify that tracked reserved and used ranges can record every current owner class.
#[allow(dead_code)]
pub fn verify_explicit_owner_coverage() -> bool {
    let mut frame_allocator = PhysicalFrameAllocator::new();
    frame_allocator.reserve_region_for(PhysAddr::new(0), 1, FrameRangeOwner::KernelImage);
    frame_allocator.reserve_region_for(PhysAddr::new(FRAME_SIZE), 1, FrameRangeOwner::Mmio);
    frame_allocator.reserve_region_for(
        PhysAddr::new(2 * FRAME_SIZE),
        1,
        FrameRangeOwner::GuardPage,
    );
    frame_allocator.add_region(PhysAddr::new(3 * FRAME_SIZE), 20);

    let allocation_plan = [
        (FrameRangeOwner::PageTable, 1),
        (FrameRangeOwner::KernelHeap, 2),
        (FrameRangeOwner::KernelStack, 2),
        (FrameRangeOwner::FramebufferBackbuffer, 2),
        (FrameRangeOwner::AhciDma, 3),
        (FrameRangeOwner::DynamicKernelMapping, 1),
        (FrameRangeOwner::UserStack, 2),
        (FrameRangeOwner::UserElf, 4),
        (FrameRangeOwner::UserHeap, 1),
        (FrameRangeOwner::UserMapping, 2),
    ];

    for (owner, pages) in allocation_plan {
        if frame_allocator.allocate_frames_for(pages, owner).is_none() {
            return false;
        }
    }

    frame_allocator.owner_statistics()
        == FrameAllocatorOwnerStatistics {
            free: 0,
            firmware_reserved: 0,
            kernel_image: 1,
            mmio: 1,
            guard_page: 1,
            unknown_used: 0,
            page_table: 1,
            kernel_heap: 2,
            kernel_stack: 2,
            framebuffer_backbuffer: 2,
            ahci_dma: 3,
            dynamic_kernel_mapping: 1,
            user_stack: 2,
            user_elf: 4,
            user_heap: 1,
            user_mapping: 2,
        }
}
