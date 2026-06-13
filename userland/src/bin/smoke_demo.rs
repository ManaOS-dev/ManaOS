#![no_main]
#![no_std]

use core::sync::atomic::{AtomicU64, Ordering};
use mana_userland::syscall;

const STDOUT: usize = 1;
const BUFFER_LENGTH: usize = 64;
const DIRECTORY_BUFFER_BYTES: usize = core::mem::size_of::<syscall::UserDirectoryEntry>();
const ENTRY_ARGUMENT_MESSAGE: &[u8] = b"user entry arguments ok\n";
const BSS_MESSAGE: &[u8] = b"user bss ok\n";
const HEAP_MESSAGE: &[u8] = b"user heap ok\n";
const SHELL_MESSAGE: &[u8] = b"user shell ok\n";
const PASS_MESSAGE: &[u8] = b"user smoke ok\n";
const BSS_SMOKE_MARKER: u64 = 0x4d414e414f535f36;
static BSS_SMOKE_VALUE: AtomicU64 = AtomicU64::new(0);

#[no_mangle]
extern "C" fn _start(
    argument_count: usize,
    argument_values: *const *const u8,
    environment_values: *const *const u8,
) -> ! {
    if syscall::getpid() <= 0 {
        syscall::exit(10);
    }

    verify_entry_arguments(argument_count, argument_values, environment_values);
    verify_disk_file();
    verify_root_directory();
    verify_user_shell();
    verify_bss_zero_initialization();
    verify_user_heap_growth();

    let _ = syscall::write(STDOUT, PASS_MESSAGE);
    syscall::exit(0);
}

fn verify_entry_arguments(
    argument_count: usize,
    argument_values: *const *const u8,
    environment_values: *const *const u8,
) {
    if argument_count != 2 || argument_values.is_null() || environment_values.is_null() {
        syscall::exit(21);
    }
    if !argument_equals(argument_values, 0, b"/disk/bin/smoke_demo") {
        syscall::exit(22);
    }
    if !argument_equals(argument_values, 1, b"--storage-smoke") {
        syscall::exit(23);
    }
    if !argument_pointer_is_null(argument_values, 2) {
        syscall::exit(24);
    }
    if !argument_equals(environment_values, 0, b"MANAOS_BOOT=storage-smoke") {
        syscall::exit(25);
    }
    if !argument_pointer_is_null(environment_values, 1) {
        syscall::exit(26);
    }

    let _ = syscall::write(STDOUT, ENTRY_ARGUMENT_MESSAGE);
}

fn verify_bss_zero_initialization() {
    match BSS_SMOKE_VALUE.compare_exchange(0, BSS_SMOKE_MARKER, Ordering::AcqRel, Ordering::Acquire)
    {
        Ok(_) => {}
        Err(value) if value == BSS_SMOKE_MARKER => {}
        Err(_) => syscall::exit(19),
    }
    if BSS_SMOKE_VALUE.load(Ordering::Acquire) != BSS_SMOKE_MARKER {
        syscall::exit(20);
    }
    let _ = syscall::write(STDOUT, BSS_MESSAGE);
}

fn verify_user_heap_growth() {
    let initial_break = syscall::brk(0);
    if initial_break <= 0 {
        syscall::exit(30);
    }

    let initial_break = initial_break as usize;
    let requested_break = initial_break + 8192;
    if syscall::brk(requested_break) != requested_break as isize {
        syscall::exit(31);
    }

    let first_byte = initial_break as *mut u8;
    let last_byte = (requested_break - 1) as *mut u8;
    // SAFETY: A successful `brk` call mapped the heap range from the previous
    // break through `requested_break`.
    unsafe {
        first_byte.write(0x4d);
        last_byte.write(0x53);
        if first_byte.read() != 0x4d || last_byte.read() != 0x53 {
            syscall::exit(32);
        }
    }

    let _ = syscall::write(STDOUT, HEAP_MESSAGE);
}

fn verify_disk_file() {
    let path = b"/disk/hello.txt\0";
    let file_descriptor = syscall::open_with_options(path, syscall::OPEN_READ_ONLY, 0);
    if file_descriptor < 0 {
        syscall::exit(11);
    }

    let file_descriptor = file_descriptor as usize;
    if syscall::lseek(file_descriptor, 0, syscall::SEEK_SET) < 0 {
        syscall::exit(12);
    }

    let mut stat = syscall::FileStat {
        file_type: 0,
        size: 0,
        writable: 0,
    };
    if syscall::fstat(file_descriptor, &mut stat) < 0 {
        syscall::exit(13);
    }
    if stat.file_type != syscall::FILE_TYPE_REGULAR || stat.size == 0 {
        syscall::exit(14);
    }

    let mut buffer = [0_u8; BUFFER_LENGTH];
    let bytes_read = syscall::read(file_descriptor, &mut buffer);
    let _ = syscall::close(file_descriptor);
    if bytes_read <= 0 {
        syscall::exit(15);
    }
}

fn verify_user_shell() {
    let path = b"/disk/hello.txt\0";
    let file_descriptor = syscall::open_with_options(path, syscall::OPEN_READ_ONLY, 0);
    if file_descriptor < 0 {
        syscall::exit(27);
    }
    let mut buffer = [0_u8; BUFFER_LENGTH];
    let bytes_read = syscall::read(file_descriptor as usize, &mut buffer);
    let _ = syscall::close(file_descriptor as usize);
    if bytes_read <= 0 {
        syscall::exit(28);
    }

    let output = &buffer[..bytes_read as usize];
    if !contains(output, b"FAT32") {
        syscall::exit(29);
    }

    let _ = syscall::write(STDOUT, output);
    let _ = syscall::write(STDOUT, SHELL_MESSAGE);
}

fn verify_root_directory() {
    let path = b"/\0";
    let directory_descriptor = syscall::open_with_options(path, syscall::OPEN_READ_ONLY, 0);
    if directory_descriptor < 0 {
        syscall::exit(16);
    }

    let directory_descriptor = directory_descriptor as usize;
    let mut entry_bytes = core::mem::MaybeUninit::<[u8; DIRECTORY_BUFFER_BYTES]>::uninit();
    let bytes_read = syscall::syscall3(
        syscall::SYS_GETDENTS64,
        directory_descriptor,
        entry_bytes.as_mut_ptr() as usize,
        DIRECTORY_BUFFER_BYTES,
    );
    let _ = syscall::close(directory_descriptor);
    if bytes_read <= 0 {
        syscall::exit(17);
    }
    // SAFETY: A positive `getdents64` result means the kernel initialized at
    // least one fixed directory-entry record in this buffer.
    let entry_bytes = unsafe { entry_bytes.assume_init() };
    let name_length = u64::from_ne_bytes([
        entry_bytes[16],
        entry_bytes[17],
        entry_bytes[18],
        entry_bytes[19],
        entry_bytes[20],
        entry_bytes[21],
        entry_bytes[22],
        entry_bytes[23],
    ]);
    if name_length == 0 {
        syscall::exit(18);
    }
}

fn argument_equals(arguments: *const *const u8, index: usize, expected: &[u8]) -> bool {
    let Some(argument_pointer) = read_argument_pointer(arguments, index) else {
        return false;
    };
    c_string_equals(argument_pointer, expected)
}

fn argument_pointer_is_null(arguments: *const *const u8, index: usize) -> bool {
    read_argument_pointer(arguments, index).is_none()
}

fn read_argument_pointer(arguments: *const *const u8, index: usize) -> Option<*const u8> {
    // SAFETY: The kernel passes null-terminated pointer arrays in user memory.
    // The smoke test reads only the small fixed indexes it validates.
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

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    if needle.len() > haystack.len() {
        return false;
    }

    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}
