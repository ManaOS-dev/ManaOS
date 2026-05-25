//! Byte-field helpers for FAT32 structures.

use core::str;

pub(super) fn read_le_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

pub(super) fn read_le_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

pub(super) fn ascii_field(bytes: &[u8]) -> &str {
    let trimmed = bytes
        .iter()
        .rposition(|byte| *byte != b' ')
        .map_or(&[][..], |last| &bytes[..=last]);
    str::from_utf8(trimmed).unwrap_or("?")
}
