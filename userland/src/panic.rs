//! Shared panic handler for ManaOS user programs.

use crate::syscall;
use core::panic::PanicInfo;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    syscall::exit(255);
}
