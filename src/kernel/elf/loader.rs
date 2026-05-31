use crate::kernel::elf::parser::{ElfError, ElfFile, ProgramHeader};
use crate::kernel::memory::{frame_allocator::BumpFrameAllocator, user_stack};
use core::cmp::{max, min};
use x86_64::structures::paging::PageTableFlags;

const PAGE_SIZE: u64 = 4096;
const USER_SPACE_END: u64 = 0x0000_8000_0000_0000;
const PF_EXECUTE: u32 = 1;
const PF_WRITE: u32 = 2;
const USER_SMOKE_DEMO_ELF: &[u8] = include_bytes!(env!("MANAOS_USER_SMOKE_DEMO_ELF"));

/// Metadata returned after a user ELF image has been loaded.
pub struct LoadedElf {
    entry_point: u64,
}

impl LoadedElf {
    /// Return the executable entry point.
    pub fn entry_point(&self) -> u64 {
        self.entry_point
    }
}

/// Load the built user smoke demo ELF into user virtual memory.
///
/// # Panics
///
/// Panics if the built ELF image is invalid, unsupported, or cannot be mapped.
pub fn load_user_smoke_demo(frame_allocator: &mut BumpFrameAllocator) -> LoadedElf {
    let loaded = load_user_elf(frame_allocator, USER_SMOKE_DEMO_ELF)
        .unwrap_or_else(|error| panic!("failed to load user smoke ELF: {}", error.message()));
    crate::log_info!(
        "elf",
        "User ELF demo loaded: entry={:#x}",
        loaded.entry_point()
    );
    loaded
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum LoadError {
    Elf(ElfError),
    EntryOutOfRange,
    NoLoadSegments,
    SegmentAddressOverflow,
    SegmentAddressOutOfRange,
    SegmentAlignmentUnsupported,
    SegmentFileOutOfBounds,
    SegmentFileLargerThanMemory,
}

impl LoadError {
    fn message(self) -> &'static str {
        match self {
            Self::Elf(error) => error.message(),
            Self::EntryOutOfRange => "ELF entry point is outside user space",
            Self::NoLoadSegments => "ELF contains no PT_LOAD segments",
            Self::SegmentAddressOverflow => "ELF segment address overflowed",
            Self::SegmentAddressOutOfRange => "ELF segment is outside user space",
            Self::SegmentAlignmentUnsupported => "ELF segment alignment is unsupported",
            Self::SegmentFileOutOfBounds => "ELF segment file range is out of bounds",
            Self::SegmentFileLargerThanMemory => "ELF segment file size exceeds memory size",
        }
    }
}

fn load_user_elf(
    frame_allocator: &mut BumpFrameAllocator,
    image: &[u8],
) -> Result<LoadedElf, LoadError> {
    let elf = ElfFile::parse(image).map_err(LoadError::Elf)?;
    validate_entry_point(elf.entry())?;
    crate::log_info!(
        "elf",
        "ELF header validated: entry={:#x} ph_count={}",
        elf.entry(),
        elf.program_header_count()
    );

    let mut load_segments = 0_u16;
    for program_header in elf.program_headers() {
        let program_header = program_header.map_err(LoadError::Elf)?;
        if !program_header.is_load() {
            continue;
        }
        load_segments = load_segments.saturating_add(1);
        map_load_segment(frame_allocator, image, program_header)?;
        crate::log_info!(
            "elf",
            "ELF segment mapped: vaddr={:#x} memsz={} filesz={} flags={:#x}",
            program_header.virtual_address(),
            program_header.memory_size(),
            program_header.file_size(),
            program_header.flags()
        );
    }

    if load_segments == 0 {
        return Err(LoadError::NoLoadSegments);
    }

    Ok(LoadedElf {
        entry_point: elf.entry(),
    })
}

fn validate_entry_point(entry_point: u64) -> Result<(), LoadError> {
    if entry_point == 0 || entry_point >= USER_SPACE_END {
        return Err(LoadError::EntryOutOfRange);
    }
    Ok(())
}

fn map_load_segment(
    frame_allocator: &mut BumpFrameAllocator,
    image: &[u8],
    program_header: ProgramHeader,
) -> Result<(), LoadError> {
    if program_header.memory_size() == 0 {
        return Ok(());
    }
    if program_header.file_size() > program_header.memory_size() {
        return Err(LoadError::SegmentFileLargerThanMemory);
    }
    if !has_supported_alignment(program_header) {
        return Err(LoadError::SegmentAlignmentUnsupported);
    }

    let segment_start = program_header.virtual_address();
    let memory_end = segment_start
        .checked_add(program_header.memory_size())
        .ok_or(LoadError::SegmentAddressOverflow)?;
    let file_end = program_header
        .offset()
        .checked_add(program_header.file_size())
        .ok_or(LoadError::SegmentFileOutOfBounds)?;
    if memory_end > USER_SPACE_END {
        return Err(LoadError::SegmentAddressOutOfRange);
    }
    if usize::try_from(file_end).map_or(true, |end| end > image.len()) {
        return Err(LoadError::SegmentFileOutOfBounds);
    }

    let first_page = align_down_to_page(segment_start);
    let last_page = align_down_to_page(memory_end - 1);
    let page_flags = page_flags_for_segment(program_header.flags());
    let file_backed_end = segment_start
        .checked_add(program_header.file_size())
        .ok_or(LoadError::SegmentAddressOverflow)?;

    let mut page_start = first_page;
    loop {
        let physical_address =
            user_stack::allocate_and_map_user_page(frame_allocator, page_start, page_flags);
        copy_segment_page(
            image,
            program_header,
            page_start,
            file_backed_end,
            physical_address,
        )?;

        if page_start == last_page {
            break;
        }
        page_start = page_start
            .checked_add(PAGE_SIZE)
            .ok_or(LoadError::SegmentAddressOverflow)?;
    }

    Ok(())
}

fn has_supported_alignment(program_header: ProgramHeader) -> bool {
    program_header.virtual_address() % PAGE_SIZE == program_header.offset() % PAGE_SIZE
        && (program_header.alignment() <= 1 || program_header.alignment() >= PAGE_SIZE)
}

fn copy_segment_page(
    image: &[u8],
    program_header: ProgramHeader,
    page_start: u64,
    file_backed_end: u64,
    physical_address: u64,
) -> Result<(), LoadError> {
    let copy_start = max(page_start, program_header.virtual_address());
    let copy_end = min(
        page_start
            .checked_add(PAGE_SIZE)
            .ok_or(LoadError::SegmentAddressOverflow)?,
        file_backed_end,
    );
    if copy_start >= copy_end {
        return Ok(());
    }

    let source_offset = program_header
        .offset()
        .checked_add(copy_start - program_header.virtual_address())
        .ok_or(LoadError::SegmentFileOutOfBounds)?;
    let copy_length =
        usize::try_from(copy_end - copy_start).map_err(|_| LoadError::SegmentFileOutOfBounds)?;
    let source_start =
        usize::try_from(source_offset).map_err(|_| LoadError::SegmentFileOutOfBounds)?;
    let source_end = source_start
        .checked_add(copy_length)
        .ok_or(LoadError::SegmentFileOutOfBounds)?;
    let source = image
        .get(source_start..source_end)
        .ok_or(LoadError::SegmentFileOutOfBounds)?;
    let destination_offset =
        usize::try_from(copy_start - page_start).map_err(|_| LoadError::SegmentAddressOverflow)?;
    let destination = (physical_address as *mut u8).wrapping_add(destination_offset);

    // SAFETY: `destination` points inside the freshly allocated page returned by
    // allocate_and_map_user_page, and `source` was bounds-checked above.
    unsafe {
        core::ptr::copy_nonoverlapping(source.as_ptr(), destination, copy_length);
    }
    Ok(())
}

fn page_flags_for_segment(segment_flags: u32) -> PageTableFlags {
    let mut page_flags = PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;
    if segment_flags & PF_WRITE != 0 {
        page_flags |= PageTableFlags::WRITABLE;
    }
    if segment_flags & PF_EXECUTE == 0 {
        page_flags |= PageTableFlags::NO_EXECUTE;
    }
    page_flags
}

fn align_down_to_page(address: u64) -> u64 {
    address & !(PAGE_SIZE - 1)
}
