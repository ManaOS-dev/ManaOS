//! Static user virtual address layout.

use super::address::UserVirtualAddress;

const PAGE_SIZE: u64 = 4096;

/// Virtual base used by linked user executables.
pub const USER_PROGRAM_BASE: u64 = 0x0000_4000_0000_0000;
/// Exclusive upper bound for the first `brk`-managed user heap model.
pub const USER_HEAP_END: u64 = 0x0000_6000_0000_0000;
/// Inclusive start of the private user mapping region.
pub const USER_MAPPING_BASE: u64 = USER_HEAP_END;
/// Exclusive end of the private user mapping region.
pub const USER_MAPPING_END: u64 = 0x0000_7000_0000_0000;
/// Inclusive start of the fixed user stack slot region.
pub const USER_STACK_REGION_BASE: u64 = 0x0000_7fff_f000_0000;
/// Bytes reserved for each fixed user stack slot.
pub const USER_STACK_SLOT_BYTES: u64 = 0x0010_0000;

const _: () = assert!(USER_HEAP_END == USER_MAPPING_BASE);
const _: () = assert!(USER_PROGRAM_BASE < USER_HEAP_END);
const _: () = assert!(USER_MAPPING_BASE < USER_MAPPING_END);
const _: () = assert!(USER_MAPPING_END < USER_STACK_REGION_BASE);
const _: () = assert!(UserVirtualAddress::new(USER_PROGRAM_BASE).is_some());
const _: () = assert!(UserVirtualAddress::new(USER_MAPPING_BASE).is_some());
const _: () = assert!(UserVirtualAddress::new(USER_MAPPING_END - PAGE_SIZE).is_some());
const _: () = assert!(UserVirtualAddress::new(USER_STACK_REGION_BASE).is_some());
