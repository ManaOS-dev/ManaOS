#![no_main]
#![no_std]

use mana_userland::syscall;

const STDIN: usize = 0;
const STDOUT: usize = 1;
const COMMAND_BUFFER_BYTES: usize = 128;
const READY_MESSAGE: &[u8] = b"user shell ready\n";
const STDIN_EOF_MESSAGE: &[u8] = b"user shell stdin eof\n";
const READ_ERROR_MESSAGE: &[u8] = b"user shell read error\n";
const INPUT_BUFFERED_MESSAGE: &[u8] = b"user shell input buffered\n";

#[no_mangle]
extern "C" fn _start() -> ! {
    let _ = syscall::write(STDOUT, READY_MESSAGE);
    let mut command_buffer = [0_u8; COMMAND_BUFFER_BYTES];
    let bytes_read = syscall::read(STDIN, &mut command_buffer);
    if bytes_read < 0 {
        let _ = syscall::write(STDOUT, READ_ERROR_MESSAGE);
        syscall::exit(1);
    }
    if bytes_read == 0 {
        let _ = syscall::write(STDOUT, STDIN_EOF_MESSAGE);
        syscall::exit(0);
    }
    let _ = syscall::write(STDOUT, INPUT_BUFFERED_MESSAGE);
    syscall::exit(0);
}
