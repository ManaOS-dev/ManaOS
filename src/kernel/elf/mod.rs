//! # `kernel::elf`
//!
//! ## Owns
//! - ELF64 executable validation for built-in user programs
//! - `PT_LOAD` segment mapping into user virtual memory
//!
//! ## Does NOT own
//! - User task scheduling (-> `kernel::task`)
//! - Physical frame allocation policy (-> `kernel::memory::frame_allocator`)
//!
//! ## Public API
//! - [`load_user_smoke_demo`] - Load the built smoke demo ELF into user memory
//! - [`LoadedElf`] - Entry metadata for a loaded user executable

mod loader;
mod parser;

pub use loader::{load_user_smoke_demo, LoadedElf};
