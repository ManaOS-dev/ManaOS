//! # `kernel::memory`
//!
//! ## Owns
//! - Physical frame allocation
//! - Kernel virtual range reservation
//! - Kernel heap initialization
//! - Early paging setup
//!
//! ## Does NOT own
//! - Architecture interrupt setup (-> `arch`)
//! - Device drivers (-> `kernel::driver`)
//!
//! ## Public API
//! - [`address_space`] - User address-space page-table roots
//! - [`address`] - Typed memory address wrappers
//! - [`diagnostics`] - Physical frame allocator diagnostics snapshots
//! - [`frame_allocator`] - Reusable physical frame allocator
//! - [`heap`] - Kernel heap allocator
//! - [`paging`] - Page table setup
//! - [`user_pointer`] - User pointer validation and copy helpers
//! - [`user_stack`] - User-space stack mapping
//! - [`virtual_allocator`] - Kernel virtual address range allocator

pub mod address;
pub mod address_space;
pub mod diagnostics;
pub mod frame_allocator;
pub mod heap;
pub mod paging;
pub mod user_pointer;
pub mod user_stack;
pub mod virtual_allocator;
