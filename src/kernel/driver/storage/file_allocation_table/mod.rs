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

mod parser;

pub(super) use parser::inspect_boot_sector;
