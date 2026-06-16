//! Scheduler user-memory syscall helpers.

use super::{
    PhysicalFrameAllocator, Scheduler, TaskKind, UserMappingError, UserMappingPlan,
    UserMappingRequest, UserMappingSource, UserMappingUnmapRequest,
};
use crate::kernel::memory::user_heap::UserHeapBreakRequest;

impl Scheduler {
    pub(in crate::kernel::task::scheduler) fn process_current_user_break(
        &mut self,
        frame_allocator: &mut PhysicalFrameAllocator,
        request: UserHeapBreakRequest,
    ) -> Option<u64> {
        let current_task = &mut self.tasks[self.current_index];
        let task_id = current_task.get_id();
        let TaskKind::User(user_runtime) = &mut current_task.kind else {
            return None;
        };
        let address_space = user_runtime.address_space?;
        let previous_break = user_runtime.heap.current_break();
        let next_break = user_runtime
            .heap
            .process_break(address_space, frame_allocator, request);
        crate::log_info!(
            "syscall",
            "brk -> task={} requested={:#x} heap_base={:#x} previous={:#x} next={:#x} mapped_end={:#x} mapped_pages={} brk_request_typed=true",
            task_id,
            request.as_u64(),
            user_runtime.heap.base().as_u64(),
            previous_break.as_u64(),
            next_break.as_u64(),
            user_runtime.heap.mapped_end().as_u64(),
            user_runtime.heap.mapped_pages()
        );
        Some(next_break.as_u64())
    }

    pub(in crate::kernel::task::scheduler) fn process_current_user_mapping(
        &mut self,
        frame_allocator: &mut PhysicalFrameAllocator,
        request: UserMappingRequest,
        initialize_page: impl FnMut(u64, &mut [u8]) -> Result<(), UserMappingError>,
    ) -> Result<u64, UserMappingError> {
        let current_task = &mut self.tasks[self.current_index];
        let task_id = current_task.get_id();
        let TaskKind::User(user_runtime) = &mut current_task.kind else {
            return Err(UserMappingError::InvalidRequest);
        };
        let address_space = user_runtime
            .address_space
            .ok_or(UserMappingError::InvalidRequest)?;
        let allocation = user_runtime.mappings.map_private(
            address_space,
            frame_allocator,
            UserMappingPlan::new(
                request.placement(),
                request.length(),
                request.writable(),
                request.source(),
            ),
            initialize_page,
        )?;
        let mapped_page_count = allocation.page_count().as_u64();
        user_runtime.mapping_total_mapped_pages = user_runtime
            .mapping_total_mapped_pages
            .saturating_add(mapped_page_count);
        user_runtime.mapping_total_released_pages = user_runtime
            .mapping_total_released_pages
            .saturating_add(allocation.replaced_page_count());
        user_runtime.mapping_peak_active_pages = user_runtime
            .mapping_peak_active_pages
            .max(user_runtime.mappings.active_pages());
        user_runtime.mapping_peak_active_records = user_runtime
            .mapping_peak_active_records
            .max(user_runtime.mappings.active_records());
        if request.source() == UserMappingSource::FilePrivate {
            user_runtime.mapping_file_private_map_count = user_runtime
                .mapping_file_private_map_count
                .saturating_add(1);
        }
        crate::log_info!(
            "syscall",
            "mmap -> task={} requested={:#x} start={:#x} length={} pages={} protection={:#x} flags={:#x} placement={} source={} active_pages={} file_private_records={} page_count_typed=true mapping_request_typed=true mapping_start_typed=true",
            task_id,
            request.requested_address_for_diagnostics(),
            allocation.start().as_u64(),
            request.length(),
            mapped_page_count,
            request.protection(),
            request.flags(),
            request.placement().as_str(),
            request.source().as_str(),
            user_runtime.mappings.active_pages(),
            user_runtime.mappings.active_file_private_records()
        );
        Ok(allocation.start().as_u64())
    }

    pub(in crate::kernel::task::scheduler) fn process_current_user_unmapping(
        &mut self,
        frame_allocator: &mut PhysicalFrameAllocator,
        request: UserMappingUnmapRequest,
    ) -> Option<u64> {
        let current_task = &mut self.tasks[self.current_index];
        let task_id = current_task.get_id();
        let TaskKind::User(user_runtime) = &mut current_task.kind else {
            return None;
        };
        let address_space = user_runtime.address_space?;
        let unmapped_pages =
            user_runtime
                .mappings
                .unmap_range(address_space, frame_allocator, request)?;
        let unmapped_page_count = unmapped_pages.as_u64();
        user_runtime.mapping_total_released_pages = user_runtime
            .mapping_total_released_pages
            .saturating_add(unmapped_page_count);
        user_runtime.mapping_peak_active_pages = user_runtime
            .mapping_peak_active_pages
            .max(user_runtime.mappings.active_pages());
        user_runtime.mapping_peak_active_records = user_runtime
            .mapping_peak_active_records
            .max(user_runtime.mappings.active_records());
        crate::log_info!(
            "syscall",
            "munmap -> task={} start={:#x} length={} pages={} unmapped=true active_pages={} active_records={} unmap_request_typed=true page_count_typed=true",
            task_id,
            request.start().as_u64(),
            request.length(),
            unmapped_page_count,
            user_runtime.mappings.active_pages(),
            user_runtime.mappings.active_records()
        );
        Some(unmapped_page_count)
    }
}
