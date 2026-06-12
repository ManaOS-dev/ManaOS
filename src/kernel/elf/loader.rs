use crate::kernel::elf::parser::{ElfError, ElfFile, ProgramHeader};
use crate::kernel::memory::{
    address::UserVirtualAddress, frame_allocator::BumpFrameAllocator, user_stack,
};
use core::cmp::{max, min};
use x86_64::structures::paging::PageTableFlags;

const PAGE_SIZE: u64 = 4096;
const PF_EXECUTE: u32 = 1;
const PF_WRITE: u32 = 2;
const PF_READ: u32 = 4;
const INVALID_ELF_REJECTION_CASES: usize = 10;

/// Metadata returned after a user ELF image has been loaded.
pub struct LoadedElf {
    entry_point: UserVirtualAddress,
}

impl LoadedElf {
    /// Return the executable entry point.
    pub fn entry_point(&self) -> UserVirtualAddress {
        self.entry_point
    }
}

/// Load a user ELF image into user virtual memory.
///
/// # Panics
///
/// Panics if the ELF image is invalid, unsupported, or cannot be mapped.
pub fn load_user_program(
    frame_allocator: &mut BumpFrameAllocator,
    image: &[u8],
    source_path: &str,
) -> LoadedElf {
    let loaded = load_user_elf(frame_allocator, image)
        .unwrap_or_else(|error| panic!("failed to load user ELF: {}", error.message()));
    crate::log_info!(
        "elf",
        "User ELF demo loaded: path={} entry={:#x}",
        source_path,
        loaded.entry_point().as_u64()
    );
    loaded
}

/// Verify that representative malformed ELF images are rejected.
pub fn verify_invalid_elf_rejections() -> bool {
    let mut passed = 0_usize;

    if rejects_mutated_elf(|image| image[0] = 0) {
        passed += 1;
    }
    if rejects_mutated_elf(|image| image[4] = 1) {
        passed += 1;
    }
    if rejects_mutated_elf(|image| image[5] = 2) {
        passed += 1;
    }
    if rejects_mutated_elf(|image| write_u16(image, 18, 0x28)) {
        passed += 1;
    }
    if rejects_mutated_elf(|image| write_u64(image, 32, 512)) {
        passed += 1;
    }
    if rejects_mutated_elf(|image| write_u64(image, 64 + 40, 32)) {
        passed += 1;
    }
    if rejects_mutated_elf(|image| write_u64(image, 64 + 48, 24)) {
        passed += 1;
    }
    if rejects_mutated_elf(|image| write_u64(image, 24, user_stack::USER_PROGRAM_BASE + PAGE_SIZE))
    {
        passed += 1;
    }
    if rejects_mutated_elf(|image| write_u32(image, 64 + 4, PF_READ | PF_WRITE | PF_EXECUTE)) {
        passed += 1;
    }
    if rejects_mutated_elf(|image| write_u32(image, 64 + 4, PF_EXECUTE)) {
        passed += 1;
    }

    if passed == INVALID_ELF_REJECTION_CASES {
        crate::log_info!(
            "elf",
            "Invalid ELF rejection smoke passed: cases={}",
            passed
        );
        true
    } else {
        crate::log_error!(
            "elf",
            "Invalid ELF rejection smoke failed: passed={} expected={}",
            passed,
            INVALID_ELF_REJECTION_CASES
        );
        false
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum LoadError {
    Elf(ElfError),
    EntryOutOfRange,
    NoLoadSegments,
    SegmentAddressOverflow,
    SegmentAddressOutOfRange,
    SegmentAlignmentUnsupported,
    SegmentPermissionUnsupported,
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
            Self::SegmentPermissionUnsupported => "ELF segment permission is unsupported",
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
    let mut entry_segment_found = false;
    for program_header in elf.program_headers() {
        let program_header = program_header.map_err(LoadError::Elf)?;
        if !program_header.is_load() {
            continue;
        }
        validate_load_segment(image, program_header)?;
        if executable_segment_contains_entry(program_header, elf.entry()) {
            entry_segment_found = true;
        }
        load_segments = load_segments.saturating_add(1);
        map_load_segment(frame_allocator, image, program_header)?;
        crate::log_info!(
            "elf",
            "ELF segment mapped: vaddr={:#x} memsz={} filesz={} flags={:#x} perms={}",
            program_header.virtual_address(),
            program_header.memory_size(),
            program_header.file_size(),
            program_header.flags(),
            segment_permission_label(program_header.flags())
        );
    }

    if load_segments == 0 {
        return Err(LoadError::NoLoadSegments);
    }
    if !entry_segment_found {
        return Err(LoadError::EntryOutOfRange);
    }

    Ok(LoadedElf {
        entry_point: UserVirtualAddress::new(elf.entry())
            .expect("validated ELF entry point must be a valid user address"),
    })
}

fn validate_entry_point(entry_point: u64) -> Result<(), LoadError> {
    if UserVirtualAddress::new(entry_point).is_none() {
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
    validate_load_segment(image, program_header)?;

    let segment_start = program_header.virtual_address();
    let memory_end = segment_start
        .checked_add(program_header.memory_size())
        .ok_or(LoadError::SegmentAddressOverflow)?;
    let file_end = program_header
        .offset()
        .checked_add(program_header.file_size())
        .ok_or(LoadError::SegmentFileOutOfBounds)?;
    if UserVirtualAddress::new(memory_end - 1).is_none() {
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
        let user_page_start =
            UserVirtualAddress::new(page_start).ok_or(LoadError::SegmentAddressOutOfRange)?;
        let physical_address =
            user_stack::allocate_and_map_user_page(frame_allocator, user_page_start, page_flags);
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

fn validate_load_segment(image: &[u8], program_header: ProgramHeader) -> Result<(), LoadError> {
    if program_header.memory_size() == 0 {
        return Ok(());
    }
    if program_header.file_size() > program_header.memory_size() {
        return Err(LoadError::SegmentFileLargerThanMemory);
    }
    if !has_supported_alignment(program_header) {
        return Err(LoadError::SegmentAlignmentUnsupported);
    }
    if !has_supported_permissions(program_header.flags()) {
        return Err(LoadError::SegmentPermissionUnsupported);
    }

    let memory_end = program_header
        .virtual_address()
        .checked_add(program_header.memory_size())
        .ok_or(LoadError::SegmentAddressOverflow)?;
    let file_end = program_header
        .offset()
        .checked_add(program_header.file_size())
        .ok_or(LoadError::SegmentFileOutOfBounds)?;
    if UserVirtualAddress::new(memory_end - 1).is_none() {
        return Err(LoadError::SegmentAddressOutOfRange);
    }
    if usize::try_from(file_end).map_or(true, |end| end > image.len()) {
        return Err(LoadError::SegmentFileOutOfBounds);
    }

    Ok(())
}

fn has_supported_alignment(program_header: ProgramHeader) -> bool {
    let alignment = program_header.alignment();
    let valid_alignment = alignment <= 1 || alignment.is_power_of_two();
    valid_alignment
        && program_header.virtual_address().is_multiple_of(PAGE_SIZE)
        && program_header.offset().is_multiple_of(PAGE_SIZE)
        && (alignment <= 1
            || program_header.virtual_address() % alignment == program_header.offset() % alignment)
}

fn has_supported_permissions(segment_flags: u32) -> bool {
    let readable = segment_flags & PF_READ != 0;
    let writable = segment_flags & PF_WRITE != 0;
    let executable = segment_flags & PF_EXECUTE != 0;
    readable && !(writable && executable)
}

fn executable_segment_contains_entry(program_header: ProgramHeader, entry_point: u64) -> bool {
    if program_header.flags() & PF_EXECUTE == 0 {
        return false;
    }
    let segment_start = program_header.virtual_address();
    let Some(segment_end) = segment_start.checked_add(program_header.memory_size()) else {
        return false;
    };
    entry_point >= segment_start && entry_point < segment_end
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

fn segment_permission_label(segment_flags: u32) -> &'static str {
    match (
        segment_flags & PF_READ != 0,
        segment_flags & PF_WRITE != 0,
        segment_flags & PF_EXECUTE != 0,
    ) {
        (true, false, true) => "R-X",
        (true, true, false) => "RW-",
        (true, false, false) => "R--",
        _ => "unsupported",
    }
}

fn align_down_to_page(address: u64) -> u64 {
    address & !(PAGE_SIZE - 1)
}

fn rejects_mutated_elf(mutate: impl FnOnce(&mut [u8])) -> bool {
    let mut image = valid_minimal_elf();
    mutate(&mut image);
    load_user_elf_metadata(&image).is_err()
}

fn load_user_elf_metadata(image: &[u8]) -> Result<LoadedElf, LoadError> {
    let elf = ElfFile::parse(image).map_err(LoadError::Elf)?;
    validate_entry_point(elf.entry())?;
    let mut load_segments = 0_u16;
    let mut entry_segment_found = false;
    for program_header in elf.program_headers() {
        let program_header = program_header.map_err(LoadError::Elf)?;
        if !program_header.is_load() {
            continue;
        }
        validate_load_segment(image, program_header)?;
        if executable_segment_contains_entry(program_header, elf.entry()) {
            entry_segment_found = true;
        }
        load_segments = load_segments.saturating_add(1);
    }
    if load_segments == 0 {
        return Err(LoadError::NoLoadSegments);
    }
    if !entry_segment_found {
        return Err(LoadError::EntryOutOfRange);
    }
    Ok(LoadedElf {
        entry_point: UserVirtualAddress::new(elf.entry())
            .expect("validated ELF entry point must be a valid user address"),
    })
}

fn valid_minimal_elf() -> [u8; 128] {
    let mut image = [0_u8; 128];
    image[0..4].copy_from_slice(b"\x7fELF");
    image[4] = 2;
    image[5] = 1;
    image[6] = 1;
    write_u16(&mut image, 16, 2);
    write_u16(&mut image, 18, 0x3e);
    write_u32(&mut image, 20, 1);
    write_u64(&mut image, 24, user_stack::USER_PROGRAM_BASE);
    write_u64(&mut image, 32, 64);
    write_u16(&mut image, 52, 64);
    write_u16(&mut image, 54, 56);
    write_u16(&mut image, 56, 1);
    write_u32(&mut image, 64, 1);
    write_u32(&mut image, 64 + 4, PF_READ | PF_EXECUTE);
    write_u64(&mut image, 64 + 8, 0);
    write_u64(&mut image, 64 + 16, user_stack::USER_PROGRAM_BASE);
    write_u64(&mut image, 64 + 32, 128);
    write_u64(&mut image, 64 + 40, PAGE_SIZE);
    write_u64(&mut image, 64 + 48, PAGE_SIZE);
    image
}

fn write_u16(image: &mut [u8], offset: usize, value: u16) {
    image[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(image: &mut [u8], offset: usize, value: u32) {
    image[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_u64(image: &mut [u8], offset: usize, value: u64) {
    image[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
