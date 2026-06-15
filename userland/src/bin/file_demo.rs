#![no_main]
#![no_std]

use mana_userland::syscall;

const STDOUT: usize = 1;
const BUFFER_LENGTH: usize = 64;
const USER_SPAWN_CHILD_DELAY_NANOS: u64 = 5_000_000;
const SPAWN_WAIT_SLEEP_NANOS: u64 = 50_000_000;
const SPAWN_WAIT_RETRY_COUNT: usize = 16;
const NON_CHILD_PROCESS_IDENTIFIER: isize = 9999;
const WAITPID_MESSAGE: &[u8] = b"user waitpid no child ok\n";
const SPAWN_WAIT_MESSAGE: &[u8] = b"user spawn waitpid ok\n";
const SPAWN_WAIT_ARGUMENT: &[u8] = b"--spawn-wait-smoke";

#[no_mangle]
extern "C" fn _start(argument_count: usize, argument_values: *const *const u8) -> ! {
    if syscall::getpid() <= 0 {
        syscall::exit(4);
    }
    let parent_task_id = syscall::getppid();
    delay_when_user_spawned_child(parent_task_id);
    let run_spawn_wait = spawn_wait_requested(argument_count, argument_values);
    if run_spawn_wait {
        change_to_disk_directory();
        verify_spawn_child_waitpid();
    } else {
        verify_waitpid_no_child();
    }
    verify_current_working_directory();

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

fn change_to_disk_directory() {
    if syscall::chdir(b"/disk\0") != 0 {
        syscall::exit(22);
    }
}

fn delay_when_user_spawned_child(parent_task_id: isize) {
    if parent_task_id <= 0 {
        return;
    }

    let duration = syscall::Timespec {
        seconds: 0,
        nanoseconds: USER_SPAWN_CHILD_DELAY_NANOS,
    };
    if syscall::nanosleep(&duration) != 0 {
        syscall::exit(14);
    }
}

fn spawn_wait_requested(argument_count: usize, argument_values: *const *const u8) -> bool {
    argument_count == 2
        && !argument_values.is_null()
        && argument_equals(argument_values, 1, SPAWN_WAIT_ARGUMENT)
}

fn verify_spawn_child_waitpid() {
    let child_path = b"bin/file_demo\0";
    let child_task_id = syscall::spawn(child_path);
    if child_task_id <= 0 || child_task_id == syscall::getpid() {
        syscall::exit(15);
    }

    let mut wait_status = -1_i32;
    if syscall::waitpid(
        child_task_id,
        &mut wait_status as *mut i32,
        syscall::WNOHANG,
    ) != 0
    {
        syscall::exit(16);
    }
    if wait_status != -1 {
        syscall::exit(17);
    }

    for _ in 0..SPAWN_WAIT_RETRY_COUNT {
        sleep_for_child_wait();
        let wait_result = syscall::waitpid(
            child_task_id,
            &mut wait_status as *mut i32,
            syscall::WNOHANG,
        );
        if wait_result == child_task_id {
            if wait_status != 0 {
                syscall::exit(18);
            }
            let _ = syscall::write(STDOUT, SPAWN_WAIT_MESSAGE);
            return;
        }
        if wait_result != 0 {
            syscall::exit(19);
        }
    }

    syscall::exit(20);
}

fn sleep_for_child_wait() {
    let duration = syscall::Timespec {
        seconds: 0,
        nanoseconds: SPAWN_WAIT_SLEEP_NANOS,
    };
    if syscall::nanosleep(&duration) != 0 {
        syscall::exit(21);
    }
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

fn argument_equals(arguments: *const *const u8, index: usize, expected: &[u8]) -> bool {
    let Some(argument_pointer) = read_argument_pointer(arguments, index) else {
        return false;
    };
    c_string_equals(argument_pointer, expected)
}

fn read_argument_pointer(arguments: *const *const u8, index: usize) -> Option<*const u8> {
    // SAFETY: The kernel passes null-terminated pointer arrays in user memory.
    // This smoke test reads only the fixed marker argument slot it validates.
    let argument_pointer = unsafe { arguments.add(index).read() };
    if argument_pointer.is_null() {
        None
    } else {
        Some(argument_pointer)
    }
}

fn c_string_equals(pointer: *const u8, expected: &[u8]) -> bool {
    for (index, expected_byte) in expected.iter().enumerate() {
        // SAFETY: The kernel provided this pointer to a NUL-terminated string
        // on the mapped user stack, and this loop reads only expected bytes.
        let actual_byte = unsafe { pointer.add(index).read() };
        if actual_byte != *expected_byte {
            return false;
        }
    }

    // SAFETY: The kernel writes a trailing NUL after every user entry string.
    unsafe { pointer.add(expected.len()).read() == 0 }
}

fn verify_current_working_directory() {
    let mut directory = [0_u8; 8];
    if syscall::getcwd(&mut directory[..5]) != syscall::ERROR_RANGE {
        syscall::exit(11);
    }
    if syscall::getcwd(&mut directory) != 6 {
        syscall::exit(12);
    }
    if &directory[..6] != b"/disk\0" {
        syscall::exit(13);
    }
}
