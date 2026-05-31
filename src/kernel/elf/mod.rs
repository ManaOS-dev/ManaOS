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
//! - [`load_user_program`] - Load a user ELF image into user memory
//! - [`verify_invalid_elf_rejections`] - Verify malformed ELF rejection cases
//! - [`LoadedElf`] - Entry metadata for a loaded user executable

mod loader;
mod parser;

pub use loader::{load_user_program, verify_invalid_elf_rejections, LoadedElf};
