//! # `kernel::task::context`
//!
//! ## Owns
//! - Architecture-visible task context type exports
//! - The stable public names used by scheduler and lifecycle code
//!
//! ## Does NOT own
//! - Kernel callee-saved context layout details (-> kernel)
//! - User first-entry context layout details (-> user)
//! - Full user trap frame layout details (-> `trap_frame`)
//!
//! ## Public API
//! - [`TaskContext`] - Saved kernel task context
//! - [`TaskEntry`] - Kernel task entry point type
//! - [`UserEntryArguments`] - User first-entry argument registers
//! - [`UserTaskContext`] - User first-entry transition frame
//! - [`UserTrapFrame`] - Future full user resume frame

mod kernel;
mod trap_frame;
mod user;

pub use kernel::{TaskContext, TaskEntry};
pub use user::{UserEntryArguments, UserTaskContext};

/// Full user-mode register frame required to resume a preempted user task.
pub type UserTrapFrame = trap_frame::UserTrapFrame;
