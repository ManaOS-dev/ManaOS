//! Finished user task resource reclaim records.

use super::stack::KernelStackReclaim;
use crate::kernel::memory::address_space::UserAddressSpaceReclaim;

/// Resources reclaimed after one finished user task has exited.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct FinishedUserTaskReclaim {
    address_space: Option<UserAddressSpaceReclaim>,
    kernel_stack: Option<KernelStackReclaim>,
}

impl FinishedUserTaskReclaim {
    /// Create a reclaim record for one finished user task cleanup pass.
    pub(super) const fn new(
        address_space: Option<UserAddressSpaceReclaim>,
        kernel_stack: Option<KernelStackReclaim>,
    ) -> Self {
        Self {
            address_space,
            kernel_stack,
        }
    }

    /// Return the reclaimed user address-space resources.
    pub(super) const fn address_space(self) -> Option<UserAddressSpaceReclaim> {
        self.address_space
    }

    /// Return the reclaimed user task kernel stack resources.
    pub(super) const fn kernel_stack(self) -> Option<KernelStackReclaim> {
        self.kernel_stack
    }

    /// Return whether this cleanup pass reclaimed a user address space.
    pub(super) const fn reclaimed_address_space(self) -> bool {
        self.address_space.is_some()
    }

    /// Return whether this cleanup pass reclaimed a user task kernel stack.
    pub(super) const fn reclaimed_kernel_stack(self) -> bool {
        self.kernel_stack.is_some()
    }

    /// Return whether this cleanup pass reclaimed any task-owned resources.
    pub(super) const fn reclaimed_anything(self) -> bool {
        self.reclaimed_address_space() || self.reclaimed_kernel_stack()
    }
}
