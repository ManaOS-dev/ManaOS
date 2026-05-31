//! # `kernel::driver::storage::file_allocation_table`
//!
//! ## Owns
//! - File Allocation Table 32 boot sector inspection
//! - File Allocation Table 32 layout metadata calculation
//!
//! ## Does NOT own
//! - Raw AHCI command submission
//! - GPT partition selection
//! - VFS mount policy
//!
//! ## Public API
//! - [`inspect_boot_sector`] - Inspect a partition boot sector as File Allocation Table 32 metadata
//! - [`inspect_root_directory`] - Inspect the root directory cluster
//! - [`inspect_file_contents`] - Inspect a file's first data cluster
//! - [`read_file_contents`] - Read a regular file's data clusters
//! - [`list_root_directory`] - List the root directory without mutating disk state
//! - [`find_entry_by_path`] - Traverse directory clusters by path
//! - [`plan_write`] - Plan a read-only write mutation

mod bytes;
mod display;
mod fsinfo;
mod parser;
mod range;

pub(super) use parser::{
    find_entry_by_path, inspect_boot_sector, inspect_file_contents, inspect_root_directory,
    list_root_directory, plan_write, FileAllocationTable32DirectoryEntry,
    FileAllocationTable32Volume,
};
pub(super) use range::read_file_range;
