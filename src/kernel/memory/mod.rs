//! # `kernel::memory`
//!
//! ## Owns
//! - Physical frame allocation
//! - Kernel heap initialization
//! - Early paging setup
//!
//! ## Does NOT own
//! - Architecture interrupt setup (-> `arch`)
//! - Device drivers (-> `kernel::driver`)
//!
//! ## Public API
//! - [`address`] - Typed memory address wrappers
//! - [`frame_allocator`] - Bump frame allocator
//! - [`heap`] - Kernel heap allocator
//! - [`paging`] - Page table setup
//! - [`user_pointer`] - User pointer validation and copy helpers
//! - [`user_stack`] - User-space stack mapping

pub mod address;
pub mod frame_allocator;
pub mod heap;
pub mod paging;
pub mod user_pointer;
pub mod user_stack;
