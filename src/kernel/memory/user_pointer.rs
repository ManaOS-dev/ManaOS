//! User pointer validation and kernel copy helpers.

use alloc::{string::String, vec::Vec};

use crate::kernel::memory::{
    address::{
        UserCString, UserReadableRange, UserVirtualAddress, UserVirtualRange, UserWritableRange,
    },
    paging,
};

/// Read a mapped user range as a kernel byte slice.
pub fn copy_from_user(range: UserReadableRange) -> Option<&'static [u8]> {
    if !paging::is_user_range_mapped_readable(range) {
        return None;
    }

    let range = range.as_range();
    let user_pointer = range.start().as_usize();
    let byte_len = range.byte_len();
    // SAFETY: The range has been bounds-checked and page-table validated as
    // present user-accessible memory before creating the kernel slice.
    Some(unsafe { core::slice::from_raw_parts(user_pointer as *const u8, byte_len) })
}

/// Read a mapped writable user range as a mutable kernel byte slice.
pub fn copy_to_user(range: UserWritableRange) -> Option<&'static mut [u8]> {
    if !paging::is_user_range_mapped_writable(range) {
        return None;
    }

    let range = range.as_range();
    let user_pointer = range.start().as_usize();
    let byte_len = range.byte_len();
    // SAFETY: The range has been bounds-checked and page-table validated as
    // present writable user-accessible memory before creating the kernel slice.
    Some(unsafe { core::slice::from_raw_parts_mut(user_pointer as *mut u8, byte_len) })
}

/// Copy a NUL-terminated user string into a kernel-owned [`String`].
pub fn copy_cstr_from_user(user_string: UserCString) -> Option<String> {
    let bytes = copy_cstr_bytes_from_user(user_string)?;
    let mut value = String::new();
    for byte in bytes {
        value.push(char::from(byte));
    }

    Some(value)
}

/// Copy NUL-terminated user string bytes into a kernel-owned byte vector.
pub fn copy_cstr_bytes_from_user(user_string: UserCString) -> Option<Vec<u8>> {
    let range = user_string.as_readable_range().as_range();
    let mut value = Vec::new();
    for offset in 0..range.byte_len() {
        let offset = u64::try_from(offset).ok()?;
        let user_pointer = range.start().checked_add(offset)?;
        let byte = copy_user_byte(user_pointer)?;
        if byte == 0 {
            return Some(value);
        }
        value.push(byte);
    }

    None
}

fn copy_user_byte(user_pointer: UserVirtualAddress) -> Option<u8> {
    let range = UserVirtualRange::new(user_pointer, 1)?;
    if !paging::is_user_range_mapped_readable(UserReadableRange::new(range)) {
        return None;
    }

    // SAFETY: The single-byte user range has been page-table validated as
    // present user-accessible readable memory before reading it.
    Some(unsafe { (user_pointer.as_usize() as *const u8).read() })
}
