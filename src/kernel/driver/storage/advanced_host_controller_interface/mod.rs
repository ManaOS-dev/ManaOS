//! # `kernel::driver::storage::advanced_host_controller_interface`
//!
//! ## Owns
//! - Advanced Host Controller Interface controller initialization
//! - SATA port setup for early block reads
//! - DMA command submission for 512-byte sector reads
//!
//! ## Does NOT own
//! - PCI bus discovery
//! - GUID partition table parsing
//! - Filesystem parsing
//!
//! ## Public API
//! - [`init`] - Initialize one storage controller from its memory register base

mod block_device;
mod command;
mod completion;
mod controller;
mod dma;
mod host;
mod port;
mod probe;
mod registers;
mod service;

pub(super) use controller::init;
pub(super) use service::{
    get_primary_data_address, read_primary_blocks, read_with_primary_device, write_primary_blocks,
};
