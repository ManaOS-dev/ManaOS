//! User pointer validation and kernel copy helpers.

use alloc::string::String;

use crate::kernel::memory::paging;

const USER_SPACE_END: usize = 0x0000_8000_0000_0000;

/// Read a mapped user range as a kernel byte slice.
pub fn copy_from_user(user_pointer: usize, length: usize) -> Option<&'static [u8]> {
    if length == 0 {
        return Some(&[]);
    }

    validate_user_range(user_pointer, length)?;
    if !paging::is_user_range_mapped_readable(user_pointer, length) {
        return None;
    }

    // SAFETY: The range has been bounds-checked and page-table validated as
    // present user-accessible memory before creating the kernel slice.
    Some(unsafe { core::slice::from_raw_parts(user_pointer as *const u8, length) })
}

/// Read a mapped writable user range as a mutable kernel byte slice.
pub fn copy_to_user(user_pointer: usize, length: usize) -> Option<&'static mut [u8]> {
    if length == 0 {
        return Some(&mut []);
    }

    validate_user_range(user_pointer, length)?;
    if !paging::is_user_range_mapped_writable(user_pointer, length) {
        return None;
    }

    // SAFETY: The range has been bounds-checked and page-table validated as
    // present writable user-accessible memory before creating the kernel slice.
    Some(unsafe { core::slice::from_raw_parts_mut(user_pointer as *mut u8, length) })
}

/// Copy a NUL-terminated user string into a kernel-owned [`String`].
pub fn copy_cstr_from_user(user_pointer: usize, max_length: usize) -> Option<String> {
    let bytes = copy_from_user(user_pointer, max_length)?;

    let mut value = String::new();
    for byte in bytes {
        if *byte == 0 {
            return Some(value);
        }

        value.push(char::from(*byte));
    }

    None
}

fn validate_user_range(user_pointer: usize, length: usize) -> Option<()> {
    if length == 0 {
        return Some(());
    }

    let end = user_pointer.checked_add(length)?;
    if user_pointer == 0 || end > USER_SPACE_END {
        return None;
    }

    Some(())
}
