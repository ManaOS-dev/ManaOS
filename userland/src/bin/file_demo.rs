#![no_main]
#![no_std]

use mana_userland::syscall;

const STDOUT: usize = 1;
const BUFFER_LENGTH: usize = 64;
const USER_SPAWN_CHILD_DELAY_NANOS: u64 = 5_000_000;
// The spawn/wait child sleeps longer than the parent spin so the smoke can
// prove both pending `waitpid(WNOHANG)` and blocking wait wake-up behavior.
const USER_SPAWN_WAIT_CHILD_DELAY_NANOS: u64 = 500_000_000;
const ORPHAN_CHILD_DELAY_NANOS: u64 = 8_000_000;
// The spawn/wait parent must remain runnable long enough for a 1 kHz timer
// tick to enter the newly spawned child before the parent blocks in waitpid.
const USER_SPAWN_PARENT_SPIN_ITERATIONS: usize = 500_000;
// Nonzero by design so waitpid status encoding cannot pass by treating all
// child exits as success.
const USER_SPAWN_CHILD_EXIT_CODE: usize = 7;
const ORPHAN_CHILD_EXIT_CODE: usize = 43;
const USER_SPAWN_CHILD_WAIT_STATUS: i32 = (USER_SPAWN_CHILD_EXIT_CODE as i32) << 8;
const NON_CHILD_PROCESS_IDENTIFIER: isize = 9999;
const WAITPID_MESSAGE: &[u8] = b"user waitpid no child ok\n";
const SPAWN_WAIT_MESSAGE: &[u8] = b"user waitpid blocking nonzero ok\n";
const SPAWN_VECTORS_MESSAGE: &[u8] = b"user spawn vectors ok\n";
const ORPHAN_PARENT_MESSAGE: &[u8] = b"user parent exit child alive ok\n";
const ORPHAN_CHILD_VECTORS_MESSAGE: &[u8] = b"user orphaned child vectors ok\n";
const SPAWN_WAIT_ARGUMENT: &[u8] = b"--spawn-wait-smoke";
const ORPHAN_PARENT_ARGUMENT: &[u8] = b"--orphan-parent-smoke";
const SHELL_COMMAND_ARGUMENT: &[u8] = b"--shell-command-smoke";
const SPAWNED_CHILD_ARGUMENT: &[u8] = b"--spawned-child";
const ORPHANED_CHILD_ARGUMENT: &[u8] = b"--orphaned-child";
const SPAWNED_CHILD_ENVIRONMENT: &[u8] = b"MANAOS_CHILD=spawn";
const ORPHANED_CHILD_ENVIRONMENT: &[u8] = b"MANAOS_CHILD=orphan";

#[no_mangle]
extern "C" fn _start(
    argument_count: usize,
    argument_values: *const *const u8,
    environment_values: *const *const u8,
) -> ! {
    if syscall::getpid() <= 0 {
        syscall::exit(4);
    }
    let parent_task_id = syscall::getppid();
    delay_when_user_spawned_child(parent_task_id, argument_count, argument_values);
    verify_spawned_child_vectors(argument_count, argument_values, environment_values);
    if orphan_parent_requested(argument_count, argument_values) {
        change_to_disk_directory();
        exit_after_spawning_orphan_child();
    }
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
    syscall::exit(exit_code_for_invocation(
        parent_task_id,
        argument_count,
        argument_values,
    ));
}

fn change_to_disk_directory() {
    if syscall::chdir(b"/disk\0") != 0 {
        syscall::exit(22);
    }
}

fn delay_when_user_spawned_child(
    parent_task_id: isize,
    argument_count: usize,
    argument_values: *const *const u8,
) {
    let is_orphaned_child = orphaned_child_requested(argument_count, argument_values);
    if !is_orphaned_child && parent_task_id <= 0 {
        return;
    }

    let nanoseconds = if is_orphaned_child {
        ORPHAN_CHILD_DELAY_NANOS
    } else if spawned_child_requested(argument_count, argument_values) {
        USER_SPAWN_WAIT_CHILD_DELAY_NANOS
    } else {
        USER_SPAWN_CHILD_DELAY_NANOS
    };
    let duration = syscall::Timespec {
        seconds: 0,
        nanoseconds,
    };
    if syscall::nanosleep(&duration) != 0 {
        syscall::exit(14);
    }
}

fn exit_code_for_invocation(
    parent_task_id: isize,
    argument_count: usize,
    argument_values: *const *const u8,
) -> usize {
    if shell_command_requested(argument_count, argument_values) {
        0
    } else if orphaned_child_requested(argument_count, argument_values) {
        ORPHAN_CHILD_EXIT_CODE
    } else if parent_task_id <= 0 {
        0
    } else {
        USER_SPAWN_CHILD_EXIT_CODE
    }
}

fn spawn_wait_requested(argument_count: usize, argument_values: *const *const u8) -> bool {
    argument_count == 2
        && !argument_values.is_null()
        && argument_equals(argument_values, 1, SPAWN_WAIT_ARGUMENT)
}

fn orphan_parent_requested(argument_count: usize, argument_values: *const *const u8) -> bool {
    argument_count == 2
        && !argument_values.is_null()
        && argument_equals(argument_values, 1, ORPHAN_PARENT_ARGUMENT)
}

fn shell_command_requested(argument_count: usize, argument_values: *const *const u8) -> bool {
    child_argument_requested(argument_count, argument_values, SHELL_COMMAND_ARGUMENT)
}

fn verify_spawn_child_waitpid() {
    let child_path = b"bin/file_demo\0";
    let child_argument0 = b"bin/file_demo\0";
    let child_argument1 = b"--spawned-child\0";
    let child_arguments: [*const u8; 3] = [
        child_argument0.as_ptr(),
        child_argument1.as_ptr(),
        core::ptr::null(),
    ];
    let child_environment0 = b"MANAOS_CHILD=spawn\0";
    let child_environment: [*const u8; 2] = [child_environment0.as_ptr(), core::ptr::null()];
    let child_task_id = syscall::spawn_with_vectors(
        child_path,
        child_arguments.as_ptr(),
        child_environment.as_ptr(),
    );
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
    spin_after_spawning_child_for_timer_preemption();

    let wait_result = syscall::waitpid(syscall::WAIT_ANY, &mut wait_status as *mut i32, 0);
    if wait_result != child_task_id {
        syscall::exit(18);
    }
    if wait_status != USER_SPAWN_CHILD_WAIT_STATUS {
        syscall::exit(19);
    }
    let _ = syscall::write(STDOUT, SPAWN_WAIT_MESSAGE);
}

fn spin_after_spawning_child_for_timer_preemption() {
    let mut remaining = USER_SPAWN_PARENT_SPIN_ITERATIONS;
    while remaining > 0 {
        core::hint::spin_loop();
        remaining -= 1;
    }
}

fn exit_after_spawning_orphan_child() -> ! {
    let child_path = b"bin/file_demo\0";
    let child_argument0 = b"bin/file_demo\0";
    let child_argument1 = b"--orphaned-child\0";
    let child_arguments: [*const u8; 3] = [
        child_argument0.as_ptr(),
        child_argument1.as_ptr(),
        core::ptr::null(),
    ];
    let child_environment0 = b"MANAOS_CHILD=orphan\0";
    let child_environment: [*const u8; 2] = [child_environment0.as_ptr(), core::ptr::null()];
    let child_task_id = syscall::spawn_with_vectors(
        child_path,
        child_arguments.as_ptr(),
        child_environment.as_ptr(),
    );
    if child_task_id <= 0 || child_task_id == syscall::getpid() {
        syscall::exit(29);
    }

    let _ = syscall::write(STDOUT, ORPHAN_PARENT_MESSAGE);
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

fn verify_spawned_child_vectors(
    argument_count: usize,
    argument_values: *const *const u8,
    environment_values: *const *const u8,
) {
    let Some(expected) = expected_child_vectors(argument_count, argument_values) else {
        return;
    };
    if argument_count != 2 || argument_values.is_null() || environment_values.is_null() {
        syscall::exit(23);
    }
    if !argument_equals(argument_values, 0, b"bin/file_demo") {
        syscall::exit(24);
    }
    if !argument_equals(argument_values, 1, expected.argument) {
        syscall::exit(25);
    }
    if read_argument_pointer(argument_values, 2).is_some() {
        syscall::exit(26);
    }
    if !argument_equals(environment_values, 0, expected.environment) {
        syscall::exit(27);
    }
    if read_argument_pointer(environment_values, 1).is_some() {
        syscall::exit(28);
    }

    let _ = syscall::write(STDOUT, expected.message);
}

struct ExpectedChildVectors {
    argument: &'static [u8],
    environment: &'static [u8],
    message: &'static [u8],
}

fn expected_child_vectors(
    argument_count: usize,
    argument_values: *const *const u8,
) -> Option<ExpectedChildVectors> {
    if spawned_child_requested(argument_count, argument_values) {
        Some(ExpectedChildVectors {
            argument: SPAWNED_CHILD_ARGUMENT,
            environment: SPAWNED_CHILD_ENVIRONMENT,
            message: SPAWN_VECTORS_MESSAGE,
        })
    } else if orphaned_child_requested(argument_count, argument_values) {
        Some(ExpectedChildVectors {
            argument: ORPHANED_CHILD_ARGUMENT,
            environment: ORPHANED_CHILD_ENVIRONMENT,
            message: ORPHAN_CHILD_VECTORS_MESSAGE,
        })
    } else {
        None
    }
}

fn spawned_child_requested(argument_count: usize, argument_values: *const *const u8) -> bool {
    child_argument_requested(argument_count, argument_values, SPAWNED_CHILD_ARGUMENT)
}

fn orphaned_child_requested(argument_count: usize, argument_values: *const *const u8) -> bool {
    child_argument_requested(argument_count, argument_values, ORPHANED_CHILD_ARGUMENT)
}

fn child_argument_requested(
    argument_count: usize,
    argument_values: *const *const u8,
    expected: &[u8],
) -> bool {
    argument_count == 2
        && !argument_values.is_null()
        && argument_equals(argument_values, 1, expected)
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
