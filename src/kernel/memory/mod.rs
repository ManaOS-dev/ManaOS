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
//! - [`frame_allocator`] - Bump frame allocator
//! - [`heap`] - Kernel heap allocator
//! - [`paging`] - Page table setup

pub mod frame_allocator;
pub mod heap;
pub mod paging;
