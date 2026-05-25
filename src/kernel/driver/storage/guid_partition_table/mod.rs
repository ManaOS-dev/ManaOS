//! # `kernel::driver::storage::guid_partition_table`
//!
//! ## Owns
//! - GUID partition table header inspection
//! - GUID partition table partition entry parsing
//! - Selection of a preferred partition entry by name or type GUID
//!
//! ## Does NOT own
//! - Storage controller command submission
//! - Block device ownership
//! - Filesystem parsing
//!
//! ## Public API
//! - [`inspect_header_with_fallback`] - Inspect primary and backup partition table headers
//! - [`inspect_partition_table`] - Inspect partition entry sectors through a block device

mod bytes;
mod display;
mod parser;

pub(super) use parser::{
    inspect_header_with_fallback, inspect_partition_table, GuidPartitionTablePartition,
};
