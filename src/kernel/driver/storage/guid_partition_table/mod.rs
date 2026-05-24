//! # `kernel::driver::storage::guid_partition_table`
//!
//! ## Owns
//! - GUID partition table header inspection
//! - GUID partition table partition entry parsing
//! - Selection of the first non-empty partition entry
//!
//! ## Does NOT own
//! - Storage controller command submission
//! - Block device ownership
//! - Filesystem parsing
//!
//! ## Public API
//! - [`inspect_header`] - Inspect a 512-byte sector as a partition table header
//! - [`inspect_partition_table`] - Inspect partition entry sectors through a block device

mod parser;

pub(super) use parser::{inspect_header, inspect_partition_table, GuidPartitionTablePartition};
