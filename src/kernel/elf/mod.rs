//! # `kernel::elf`
//!
//! ## Owns
//! - ELF64 executable validation for user programs
//! - `PT_LOAD` segment mapping into user virtual memory
//!
//! ## Does NOT own
//! - User task scheduling (-> `kernel::task`)
//! - Physical frame allocation policy (-> `kernel::memory::frame_allocator`)
//!
//! ## Public API
//! - [`validate_user_program_image`] - Validate a user ELF image without mapping it
//! - [`load_user_program`] - Load a user ELF image into user memory
//! - [`verify_invalid_elf_rejections`] - Verify malformed ELF rejection cases
//! - [`LoadedElf`] - Entry metadata for a loaded user executable

mod loader;
mod parser;

pub use loader::{
    load_user_program, validate_user_program_image, verify_invalid_elf_rejections, LoadedElf,
};
