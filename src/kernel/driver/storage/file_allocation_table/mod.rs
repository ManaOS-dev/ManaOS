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

mod parser;

pub(super) use parser::{
    inspect_boot_sector, inspect_file_contents, inspect_root_directory, read_file_contents,
};
