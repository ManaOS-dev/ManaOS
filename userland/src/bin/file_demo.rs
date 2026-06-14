#![no_main]
#![no_std]

use mana_userland::syscall;

const STDOUT: usize = 1;
const BUFFER_LENGTH: usize = 64;
const NON_CHILD_PROCESS_IDENTIFIER: isize = 9999;
const WAITPID_MESSAGE: &[u8] = b"user waitpid no child ok\n";

#[no_mangle]
extern "C" fn _start() -> ! {
    if syscall::getpid() <= 0 {
        syscall::exit(4);
    }
    verify_waitpid_no_child();

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

fn verify_waitpid_no_child() {
    if syscall::waitpid(syscall::WAIT_ANY, core::ptr::null_mut(), 0) != syscall::ERROR_NO_CHILD {
        syscall::exit(8);
    }
    if syscall::waitpid(syscall::WAIT_ANY, core::ptr::null_mut(), syscall::WNOHANG)
        != syscall::ERROR_NO_CHILD
    {
        syscall::exit(9);
    }
    if syscall::waitpid(NON_CHILD_PROCESS_IDENTIFIER, core::ptr::null_mut(), 0)
        != syscall::ERROR_NO_CHILD
    {
        syscall::exit(10);
    }

    let _ = syscall::write(STDOUT, WAITPID_MESSAGE);
}
