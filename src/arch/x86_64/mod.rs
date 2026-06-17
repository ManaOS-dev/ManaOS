//! # `arch::x86_64`
//!
//! ## Owns
//! - `x86_64` architecture module composition
//! - Descriptor, interrupt, timer, PCI, CPU, and context entry exports
//!
//! ## Does NOT own
//! - Kernel scheduler policy
//! - Device driver state
//! - Input event queues
//! - CPU initialization details (-> `cpu`)
//! - Assembly-backed context entry points (-> `context`)
//!
//! ## Public API
//! - [`init`] - Initialize `x86_64` architecture state
//! - [`SyscallEntryAddress`] - Typed `SYSCALL` entry target address
//! - [`enable_interrupts`] - Enable CPU interrupts after wiring
//! - [`disable_interrupts`] - Disable CPU interrupts during backend switching
//! - [`switch_context`] - Switch between saved task contexts
//! - [`switch_to_user_mode_context`] - Save a task context and enter Ring 3

mod context;
mod cpu;
pub mod global_descriptor_table;
pub mod interrupt_controller;
pub mod interrupt_descriptor_table;
pub mod interval_timer;
pub mod pci_configuration;

pub use context::{enter_user_mode_once, switch_context, switch_to_user_mode_context};
#[allow(unused_imports)]
pub use cpu::{
    disable_interrupts, enable_interrupts, has_apic, hlt_loop, init, init_syscall,
    read_timestamp_counter, SyscallEntryAddress,
};
