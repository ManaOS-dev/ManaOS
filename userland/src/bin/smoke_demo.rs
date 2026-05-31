#![no_main]
#![no_std]

use core::sync::atomic::{AtomicU64, Ordering};
use mana_userland::syscall;

const STDOUT: usize = 1;
const BUFFER_LENGTH: usize = 64;
const DIRECTORY_BUFFER_BYTES: usize = core::mem::size_of::<syscall::UserDirectoryEntry>();
const BSS_MESSAGE: &[u8] = b"user bss ok\n";
const PASS_MESSAGE: &[u8] = b"user smoke ok\n";
static BSS_SMOKE_VALUE: AtomicU64 = AtomicU64::new(0);

#[no_mangle]
extern "C" fn _start() -> ! {
    if syscall::getpid() <= 0 {
        syscall::exit(10);
    }

    verify_disk_file();
    verify_root_directory();
    verify_bss_zero_initialization();

    let _ = syscall::write(STDOUT, PASS_MESSAGE);
    syscall::exit(0);
}

fn verify_bss_zero_initialization() {
    if BSS_SMOKE_VALUE.load(Ordering::Relaxed) != 0 {
        syscall::exit(19);
    }
    BSS_SMOKE_VALUE.store(0x4d414e414f535f36, Ordering::Relaxed);
    if BSS_SMOKE_VALUE.load(Ordering::Relaxed) != 0x4d414e414f535f36 {
        syscall::exit(20);
    }
    let _ = syscall::write(STDOUT, BSS_MESSAGE);
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
