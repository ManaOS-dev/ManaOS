//! Minimal GPT header inspection.

const GPT_SIGNATURE: &[u8; 8] = b"EFI PART";
const SECTOR_BYTES: usize = 512;
const REVISION_OFFSET: usize = 8;
const HEADER_SIZE_OFFSET: usize = 12;
const CURRENT_LBA_OFFSET: usize = 24;
const BACKUP_LBA_OFFSET: usize = 32;
const FIRST_USABLE_LBA_OFFSET: usize = 40;
const LAST_USABLE_LBA_OFFSET: usize = 48;
const PARTITION_ENTRY_LBA_OFFSET: usize = 72;
const PARTITION_ENTRY_COUNT_OFFSET: usize = 80;
const PARTITION_ENTRY_SIZE_OFFSET: usize = 84;

/// Inspect a 512-byte sector as a GPT header and print key fields.
pub fn inspect_header(data_address: u64) {
    let sector = data_address as *const u8;
    // SAFETY: `data_address` points to a 512-byte DMA buffer filled from LBA1.
    let sector = unsafe { core::slice::from_raw_parts(sector, SECTOR_BYTES) };

    if &sector[0..GPT_SIGNATURE.len()] != GPT_SIGNATURE {
        crate::serial_println!("[gpt  ] Header signature not found at LBA1");
        return;
    }

    crate::serial_println!("[gpt  ] Header signature: EFI PART");
    crate::serial_println!(
        "[gpt  ] revision={:#010x} header_size={} current_lba={} backup_lba={}",
        read_le_u32(sector, REVISION_OFFSET),
        read_le_u32(sector, HEADER_SIZE_OFFSET),
        read_le_u64(sector, CURRENT_LBA_OFFSET),
        read_le_u64(sector, BACKUP_LBA_OFFSET)
    );
    crate::serial_println!(
        "[gpt  ] first_usable_lba={} last_usable_lba={}",
        read_le_u64(sector, FIRST_USABLE_LBA_OFFSET),
        read_le_u64(sector, LAST_USABLE_LBA_OFFSET)
    );
    crate::serial_println!(
        "[gpt  ] entries_lba={} entry_count={} entry_size={}",
        read_le_u64(sector, PARTITION_ENTRY_LBA_OFFSET),
        read_le_u32(sector, PARTITION_ENTRY_COUNT_OFFSET),
        read_le_u32(sector, PARTITION_ENTRY_SIZE_OFFSET)
    );
}

fn read_le_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn read_le_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
        bytes[offset + 4],
        bytes[offset + 5],
        bytes[offset + 6],
        bytes[offset + 7],
    ])
}
