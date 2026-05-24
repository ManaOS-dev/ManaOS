#![no_main]
#![no_std]

use core::panic::PanicInfo;
use mana_userland::syscall;

const BAD_USER_POINTER: usize = 0x0000_4000_0000_2000;
const BUFFER_LENGTH: usize = 64;

#[no_mangle]
extern "C" fn _start() -> ! {
    let path = b"/hello.txt\0";
    let file_descriptor = syscall::open_with_options(path, syscall::OPEN_READ_ONLY, 0);
    if file_descriptor < 0 {
        syscall::exit(1);
    }

    let result = syscall::syscall3(
        syscall::SYS_READ,
        file_descriptor as usize,
        BAD_USER_POINTER,
        BUFFER_LENGTH,
    );
    let _ = syscall::close(file_descriptor as usize);

    if result == syscall::ERROR_BAD_ADDRESS {
        syscall::exit(0);
    }

    syscall::exit(2);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    syscall::exit(255);
}
