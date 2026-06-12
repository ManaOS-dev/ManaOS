//! User pointer validation and kernel copy helpers.

use alloc::string::String;

use crate::kernel::memory::{
    address::{UserCString, UserReadableRange, UserWritableRange},
    paging,
};

/// Read a mapped user range as a kernel byte slice.
pub fn copy_from_user(range: UserReadableRange) -> Option<&'static [u8]> {
    let range = range.as_range();
    let user_pointer = range.start().as_usize();
    let byte_len = range.byte_len();
    if !paging::is_user_range_mapped_readable(user_pointer, byte_len) {
        return None;
    }

    // SAFETY: The range has been bounds-checked and page-table validated as
    // present user-accessible memory before creating the kernel slice.
    Some(unsafe { core::slice::from_raw_parts(user_pointer as *const u8, byte_len) })
}

/// Read a mapped writable user range as a mutable kernel byte slice.
pub fn copy_to_user(range: UserWritableRange) -> Option<&'static mut [u8]> {
    let range = range.as_range();
    let user_pointer = range.start().as_usize();
    let byte_len = range.byte_len();
    if !paging::is_user_range_mapped_writable(user_pointer, byte_len) {
        return None;
    }

    // SAFETY: The range has been bounds-checked and page-table validated as
    // present writable user-accessible memory before creating the kernel slice.
    Some(unsafe { core::slice::from_raw_parts_mut(user_pointer as *mut u8, byte_len) })
}

/// Copy a NUL-terminated user string into a kernel-owned [`String`].
pub fn copy_cstr_from_user(user_string: UserCString) -> Option<String> {
    let bytes = copy_from_user(user_string.as_readable_range())?;

    let mut value = String::new();
    for byte in bytes {
        if *byte == 0 {
            return Some(value);
        }

        value.push(char::from(*byte));
    }

    None
}
