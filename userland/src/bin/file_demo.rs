#![no_main]
#![no_std]

use core::panic::PanicInfo;
use mana_userland::syscall;

const STDOUT: usize = 1;
const BUFFER_LENGTH: usize = 64;

#[no_mangle]
extern "C" fn _start() -> ! {
    if syscall::getpid() <= 0 {
        syscall::exit(4);
    }

    let path = b"/disk/hello.txt\0";
    let file_descriptor = syscall::open_with_options(path, syscall::OPEN_READ_ONLY, 0);
    if file_descriptor < 0 {
        syscall::exit(1);
    }
    if syscall::lseek(file_descriptor as usize, 0, syscall::SEEK_SET) < 0 {
        syscall::exit(5);
    }
    let mut stat = syscall::FileStat {
        file_type: 0,
        size: 0,
        writable: 0,
    };
    if syscall::fstat(file_descriptor as usize, &mut stat) < 0 {
        syscall::exit(6);
    }
    if stat.file_type != syscall::FILE_TYPE_REGULAR || stat.size == 0 {
        syscall::exit(7);
    }

    let mut buffer = [0_u8; BUFFER_LENGTH];
    let bytes_read = syscall::read(file_descriptor as usize, &mut buffer);
    let _ = syscall::close(file_descriptor as usize);
    if bytes_read < 0 {
        syscall::exit(2);
    }

    let bytes_read = bytes_read as usize;
    if bytes_read > buffer.len() {
        syscall::exit(3);
    }

    let _ = syscall::syscall3(
        syscall::SYS_WRITE,
        STDOUT,
        buffer.as_ptr() as usize,
        bytes_read,
    );
    syscall::exit(0);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    syscall::exit(255);
}
