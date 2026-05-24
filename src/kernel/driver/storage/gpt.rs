//! Minimal GPT header and partition entry inspection.

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
const PARTITION_TYPE_GUID_SIZE: usize = 16;
const PARTITION_ENTRY_FIRST_LBA_OFFSET: usize = 32;
const PARTITION_ENTRY_LAST_LBA_OFFSET: usize = 40;

/// GPT header fields needed to locate partition entries.
#[derive(Clone, Copy)]
pub struct GptHeader {
    /// LBA containing the first partition entry sector.
    pub entries_lba: u64,
    /// Number of partition entries described by the header.
    pub count: u32,
    /// Size in bytes of each partition entry.
    pub size: u32,
}

/// Summary of partition entries inspected from one sector.
#[derive(Clone, Copy)]
pub struct PartitionEntryScan {
    /// Number of non-empty partition entries found in the inspected sector.
    pub non_empty_entries: u32,
}

/// Inspect a 512-byte sector as a GPT header and print key fields.
pub fn inspect_header(data_address: u64) -> Option<GptHeader> {
    let sector = data_address as *const u8;
    // SAFETY: `data_address` points to a 512-byte DMA buffer filled from LBA1.
    let sector = unsafe { core::slice::from_raw_parts(sector, SECTOR_BYTES) };

    if &sector[0..GPT_SIGNATURE.len()] != GPT_SIGNATURE {
        crate::serial_println!("[gpt  ] Header signature not found at LBA1");
        return None;
    }

    let partition_entry_lba = read_le_u64(sector, PARTITION_ENTRY_LBA_OFFSET);
    let partition_entry_count = read_le_u32(sector, PARTITION_ENTRY_COUNT_OFFSET);
    let partition_entry_size = read_le_u32(sector, PARTITION_ENTRY_SIZE_OFFSET);

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
        partition_entry_lba,
        partition_entry_count,
        partition_entry_size
    );

    Some(GptHeader {
        entries_lba: partition_entry_lba,
        count: partition_entry_count,
        size: partition_entry_size,
    })
}

/// Inspect GPT partition entries contained in one 512-byte sector.
pub fn inspect_partition_entries(
    data_address: u64,
    first_entry_index: u32,
    entry_count: u32,
    entry_size: u32,
) -> PartitionEntryScan {
    let entry_size = usize::try_from(entry_size).expect("GPT entry size must fit in usize");
    let entry_count = usize::try_from(entry_count).expect("GPT entry count must fit in usize");
    let first_entry_index =
        usize::try_from(first_entry_index).expect("GPT entry index must fit in usize");
    let sector = data_address as *const u8;
    // SAFETY: `data_address` points to a 512-byte DMA buffer filled from a GPT
    // partition entry sector.
    let sector = unsafe { core::slice::from_raw_parts(sector, SECTOR_BYTES) };
    let mut non_empty_entries = 0;

    for entry_index in 0..entry_count {
        let offset = entry_index * entry_size;
        let entry = &sector[offset..offset + entry_size];
        if is_empty_partition_entry(entry) {
            continue;
        }

        non_empty_entries += 1;
        log_partition_entry(first_entry_index + entry_index, entry);
    }

    PartitionEntryScan { non_empty_entries }
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

fn is_empty_partition_entry(entry: &[u8]) -> bool {
    entry[0..PARTITION_TYPE_GUID_SIZE]
        .iter()
        .all(|byte| *byte == 0)
}

fn log_partition_entry(entry_index: usize, entry: &[u8]) {
    crate::serial_println!(
        "[gpt  ] Partition entry {}: first_lba={} last_lba={}",
        entry_index,
        read_le_u64(entry, PARTITION_ENTRY_FIRST_LBA_OFFSET),
        read_le_u64(entry, PARTITION_ENTRY_LAST_LBA_OFFSET)
    );
}
