#![no_main]
#![no_std]

use mana_userland::syscall;

const STDOUT: usize = 1;
const READY_MESSAGE: &[u8] = b"user shell ready\n";

#[no_mangle]
extern "C" fn _start() -> ! {
    let _ = syscall::write(STDOUT, READY_MESSAGE);
    syscall::exit(0);
}
