//! GUID partition table parsing implementation.

use core::fmt;

use super::super::block_device::{BlockDevice, SECTOR_BYTES};

const GUID_PARTITION_TABLE_SIGNATURE: &[u8; 8] = b"EFI PART";
const REVISION_OFFSET: usize = 8;
const HEADER_SIZE_OFFSET: usize = 12;
const HEADER_CRC32_OFFSET: usize = 16;
const RESERVED_OFFSET: usize = 20;
const CURRENT_LBA_OFFSET: usize = 24;
const BACKUP_LBA_OFFSET: usize = 32;
const FIRST_USABLE_LBA_OFFSET: usize = 40;
const LAST_USABLE_LBA_OFFSET: usize = 48;
const DISK_GUID_OFFSET: usize = 56;
const PARTITION_ENTRY_LBA_OFFSET: usize = 72;
const PARTITION_ENTRY_COUNT_OFFSET: usize = 80;
const PARTITION_ENTRY_SIZE_OFFSET: usize = 84;
const PARTITION_ENTRY_ARRAY_CRC32_OFFSET: usize = 88;
const PARTITION_TYPE_GUID_SIZE: usize = 16;
const PARTITION_UNIQUE_GUID_OFFSET: usize = 16;
const PARTITION_ENTRY_FIRST_LBA_OFFSET: usize = 32;
const PARTITION_ENTRY_LAST_LBA_OFFSET: usize = 40;
const PARTITION_ENTRY_ATTRIBUTES_OFFSET: usize = 48;
const PARTITION_ENTRY_NAME_OFFSET: usize = 56;
const PARTITION_ENTRY_NAME_BYTES: usize = 72;
const PARTITION_NAME_CAPACITY: usize = 36;
const MINIMUM_HEADER_SIZE: usize = 92;
const CRC32_INITIAL: u32 = u32::MAX;
const CRC32_POLYNOMIAL: u32 = 0xedb8_8320;

/// GUID partition table header fields needed to locate partition entries.
#[derive(Clone, Copy)]
pub struct GuidPartitionTableHeader {
    /// LBA containing the first partition entry sector.
    pub entries_lba: u64,
    /// Number of partition entries described by the header.
    pub count: u32,
    /// Size in bytes of each partition entry.
    pub size: u32,
    /// Expected CRC32 of the partition entry array.
    pub partition_entry_array_crc32: u32,
}

/// Summary of partition entries inspected from one sector.
#[derive(Clone, Copy)]
pub struct PartitionEntryScan {
    /// Number of entries inspected in the sector.
    pub scanned: u32,
    /// Number of empty partition entries found in the inspected sector.
    pub empty: u32,
    /// Number of non-empty partition entries found in the inspected sector.
    pub non_empty: u32,
    /// First non-empty partition found in the inspected sector.
    pub first_partition: Option<GuidPartitionTablePartition>,
}

/// Parsed GUID partition table partition metadata used by storage probing.
#[derive(Clone, Copy)]
pub struct GuidPartitionTablePartition {
    /// Index in the GUID partition table partition entry array.
    pub index: u32,
    /// First usable LBA owned by this partition.
    pub first_lba: u64,
    /// Last usable LBA owned by this partition.
    pub last_lba: u64,
    /// GUID partition table partition attributes.
    pub attributes: u64,
    /// ASCII fallback partition name bytes.
    pub name: [u8; PARTITION_NAME_CAPACITY],
    /// Number of valid bytes in [`Self::name`].
    pub name_length: usize,
}

/// Inspect a 512-byte sector as a GUID partition table header and print key fields.
pub fn inspect_header(data_address: u64) -> Option<GuidPartitionTableHeader> {
    let sector = data_address as *const u8;
    // SAFETY: `data_address` points to a 512-byte DMA buffer filled from logical block address 1.
    let sector = unsafe { core::slice::from_raw_parts(sector, SECTOR_BYTES) };

    if &sector[0..GUID_PARTITION_TABLE_SIGNATURE.len()] != GUID_PARTITION_TABLE_SIGNATURE {
        crate::log_warn!(
            "gpt",
            "Header signature not found at logical block address 1"
        );
        return None;
    }

    let header_size = usize::try_from(read_le_u32(sector, HEADER_SIZE_OFFSET))
        .expect("GUID partition table header size must fit in usize");
    if !(MINIMUM_HEADER_SIZE..=SECTOR_BYTES).contains(&header_size) {
        crate::log_warn!("gpt", "Invalid header_size={}", header_size);
        return None;
    }

    let expected_header_crc32 = read_le_u32(sector, HEADER_CRC32_OFFSET);
    let actual_header_crc32 = calculate_header_crc32(sector, header_size);
    if actual_header_crc32 != expected_header_crc32 {
        crate::log_warn!(
            "gpt",
            "Header CRC32 mismatch: expected={:#010x} actual={:#010x} header_size={}",
            expected_header_crc32,
            actual_header_crc32,
            header_size
        );
        return None;
    }

    crate::log_info!("gpt", "Header signature: EFI PART");
    crate::log_info!(
        "gpt",
        "Header CRC32 validated: {:#010x}",
        actual_header_crc32
    );
    let partition_entry_lba = read_le_u64(sector, PARTITION_ENTRY_LBA_OFFSET);
    let partition_entry_count = read_le_u32(sector, PARTITION_ENTRY_COUNT_OFFSET);
    let partition_entry_size = read_le_u32(sector, PARTITION_ENTRY_SIZE_OFFSET);
    crate::log_debug!(
        "gpt",
        "header_crc32={:#010x} reserved={:#010x} partition_array_crc32={:#010x}",
        expected_header_crc32,
        read_le_u32(sector, RESERVED_OFFSET),
        read_le_u32(sector, PARTITION_ENTRY_ARRAY_CRC32_OFFSET)
    );
    crate::log_info!(
        "gpt",
        "revision={:#010x} header_size={} current_lba={} backup_lba={}",
        read_le_u32(sector, REVISION_OFFSET),
        read_le_u32(sector, HEADER_SIZE_OFFSET),
        read_le_u64(sector, CURRENT_LBA_OFFSET),
        read_le_u64(sector, BACKUP_LBA_OFFSET)
    );
    crate::log_info!(
        "gpt",
        "first_usable_lba={} last_usable_lba={}",
        read_le_u64(sector, FIRST_USABLE_LBA_OFFSET),
        read_le_u64(sector, LAST_USABLE_LBA_OFFSET)
    );
    crate::log_debug!(
        "gpt",
        "disk_guid={}",
        GuidPartitionTableGuid(
            &sector[DISK_GUID_OFFSET..DISK_GUID_OFFSET + PARTITION_TYPE_GUID_SIZE]
        )
    );
    crate::log_info!(
        "gpt",
        "entries_lba={} entry_count={} entry_size={}",
        partition_entry_lba,
        partition_entry_count,
        partition_entry_size
    );

    Some(GuidPartitionTableHeader {
        entries_lba: partition_entry_lba,
        count: partition_entry_count,
        size: partition_entry_size,
        partition_entry_array_crc32: read_le_u32(sector, PARTITION_ENTRY_ARRAY_CRC32_OFFSET),
    })
}

/// Inspect GUID partition table partition entries contained in one 512-byte sector.
pub fn inspect_partition_entries(
    data_address: u64,
    first_entry_index: u32,
    entry_count: u32,
    entry_size: u32,
) -> PartitionEntryScan {
    let entry_size =
        usize::try_from(entry_size).expect("GUID partition table entry size must fit in usize");
    let entry_count =
        usize::try_from(entry_count).expect("GUID partition table entry count must fit in usize");
    let first_entry_index = usize::try_from(first_entry_index)
        .expect("GUID partition table entry index must fit in usize");
    let sector = data_address as *const u8;
    // SAFETY: `data_address` points to a 512-byte DMA buffer filled from a GUID partition table
    // partition entry sector.
    let sector = unsafe { core::slice::from_raw_parts(sector, SECTOR_BYTES) };
    let mut non_empty_entries = 0;
    let mut empty_entries = 0;
    let mut first_partition = None;

    for entry_index in 0..entry_count {
        let offset = entry_index * entry_size;
        let entry = &sector[offset..offset + entry_size];
        if is_empty_partition_entry(entry) {
            empty_entries += 1;
            continue;
        }

        let partition = parse_partition_entry(first_entry_index + entry_index, entry);
        non_empty_entries += 1;
        if first_partition.is_none() {
            first_partition = Some(partition);
        }
        log_partition_entry(partition, entry);
    }

    PartitionEntryScan {
        scanned: u32::try_from(entry_count)
            .expect("GUID partition table scanned entry count must fit in u32"),
        empty: u32::try_from(empty_entries)
            .expect("GUID partition table empty entry count must fit in u32"),
        non_empty: u32::try_from(non_empty_entries)
            .expect("GUID partition table non-empty entry count must fit in u32"),
        first_partition,
    }
}

/// Inspect the GUID partition table partition-entry array and return the first non-empty partition.
pub(in crate::kernel::driver::storage) fn inspect_partition_table(
    block_device: &mut impl BlockDevice,
    header: GuidPartitionTableHeader,
    data_address: u64,
) -> Option<GuidPartitionTablePartition> {
    let entry_size =
        usize::try_from(header.size).expect("GUID partition table entry size must fit in usize");
    if !(48..=SECTOR_BYTES).contains(&entry_size) {
        crate::log_warn!("gpt", "Unsupported partition entry size: {}", header.size);
        return None;
    }

    let entries_per_sector = SECTOR_BYTES / entry_size;
    if entries_per_sector == 0 {
        crate::log_warn!("gpt", "Unsupported partition entry size: {}", header.size);
        return None;
    }

    let total_entry_bytes = u64::from(header.count) * u64::from(header.size);
    let sector_count = total_entry_bytes.div_ceil(SECTOR_BYTES as u64);
    let entries_per_sector_u32 =
        u32::try_from(entries_per_sector).expect("entries per sector must fit in u32");
    let mut non_empty_entries = 0;
    let mut empty_entries = 0;
    let mut entries_remaining = header.count;
    let mut first_partition = None;
    let mut partition_array_crc32 = CRC32_INITIAL;

    crate::log_debug!(
        "gpt",
        "Partition scan: start_lba={} total_entries={} entry_size={} total_bytes={}",
        header.entries_lba,
        header.count,
        header.size,
        total_entry_bytes
    );
    crate::log_debug!(
        "gpt",
        "Partition scan: sectors={} entries_per_sector={}",
        sector_count,
        entries_per_sector
    );

    for sector_offset in 0..sector_count {
        if entries_remaining == 0 {
            break;
        }

        let logical_block_address = header.entries_lba + sector_offset;
        if !block_device.read_logical_block(logical_block_address, data_address) {
            return None;
        }

        let entry_count = entries_remaining.min(entries_per_sector_u32);
        let checksum_bytes = usize::try_from(entry_count)
            .expect("GUID partition table entry count must fit in usize")
            .checked_mul(entry_size)
            .expect("GUID partition table checksum byte count must not overflow");
        let sector = data_address as *const u8;
        // SAFETY: `data_address` points to a 512-byte DMA buffer filled from a
        // GUID partition table partition entry sector.
        let sector = unsafe { core::slice::from_raw_parts(sector, SECTOR_BYTES) };
        partition_array_crc32 =
            update_crc32_bytes(partition_array_crc32, &sector[..checksum_bytes]);

        let first_entry_index = u32::try_from(sector_offset)
            .expect("GUID partition table partition entry sector offset must fit in u32")
            * entries_per_sector_u32;
        let scan =
            inspect_partition_entries(data_address, first_entry_index, entry_count, header.size);
        if first_partition.is_none() {
            first_partition = scan.first_partition;
        }
        crate::log_trace!(
            "gpt",
            "Partition scan sector: lba={} first_entry={} scanned={} empty={} non_empty={}",
            logical_block_address,
            first_entry_index,
            scan.scanned,
            scan.empty,
            scan.non_empty
        );
        empty_entries += scan.empty;
        non_empty_entries += scan.non_empty;
        entries_remaining -= entry_count;
    }

    let actual_partition_array_crc32 = finalize_crc32(partition_array_crc32);
    if actual_partition_array_crc32 != header.partition_entry_array_crc32 {
        crate::log_warn!(
            "gpt",
            "Partition array CRC32 mismatch: expected={:#010x} actual={:#010x}",
            header.partition_entry_array_crc32,
            actual_partition_array_crc32
        );
        return None;
    }
    crate::log_info!(
        "gpt",
        "Partition array CRC32 validated: {:#010x}",
        actual_partition_array_crc32
    );

    crate::log_info!(
        "gpt",
        "Partition scan summary: scanned={} empty={} non_empty={}",
        header.count,
        empty_entries,
        non_empty_entries
    );
    if non_empty_entries == 0 {
        crate::log_info!("gpt", "No partition entries found");
    } else {
        crate::log_info!("gpt", "Partition entries found: {}", non_empty_entries);
    }

    first_partition
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

fn calculate_header_crc32(sector: &[u8], header_size: usize) -> u32 {
    let mut crc32 = CRC32_INITIAL;
    let header_crc32_end = HEADER_CRC32_OFFSET + core::mem::size_of::<u32>();

    for (offset, byte) in sector[..header_size].iter().enumerate() {
        let byte = if (HEADER_CRC32_OFFSET..header_crc32_end).contains(&offset) {
            0
        } else {
            *byte
        };
        crc32 = update_crc32(crc32, byte);
    }

    finalize_crc32(crc32)
}

fn update_crc32_bytes(mut crc32: u32, bytes: &[u8]) -> u32 {
    for byte in bytes {
        crc32 = update_crc32(crc32, *byte);
    }
    crc32
}

fn finalize_crc32(crc32: u32) -> u32 {
    !crc32
}

fn update_crc32(mut crc32: u32, byte: u8) -> u32 {
    crc32 ^= u32::from(byte);
    for _ in 0..8 {
        let mask = 0_u32.wrapping_sub(crc32 & 1);
        crc32 = (crc32 >> 1) ^ (CRC32_POLYNOMIAL & mask);
    }
    crc32
}

fn is_empty_partition_entry(entry: &[u8]) -> bool {
    entry[0..PARTITION_TYPE_GUID_SIZE]
        .iter()
        .all(|byte| *byte == 0)
}

fn parse_partition_entry(entry_index: usize, entry: &[u8]) -> GuidPartitionTablePartition {
    let (name, name_length) = parse_partition_name(entry);
    GuidPartitionTablePartition {
        index: u32::try_from(entry_index)
            .expect("GUID partition table partition index must fit in u32"),
        first_lba: read_le_u64(entry, PARTITION_ENTRY_FIRST_LBA_OFFSET),
        last_lba: read_le_u64(entry, PARTITION_ENTRY_LAST_LBA_OFFSET),
        attributes: read_le_u64(entry, PARTITION_ENTRY_ATTRIBUTES_OFFSET),
        name,
        name_length,
    }
}

fn log_partition_entry(partition: GuidPartitionTablePartition, entry: &[u8]) {
    crate::log_debug!(
        "gpt",
        "Partition entry {}: type_guid={}",
        partition.index,
        GuidPartitionTableGuid(&entry[0..PARTITION_TYPE_GUID_SIZE])
    );
    crate::log_debug!(
        "gpt",
        "Partition entry {}: unique_guid={}",
        partition.index,
        GuidPartitionTableGuid(
            &entry[PARTITION_UNIQUE_GUID_OFFSET
                ..PARTITION_UNIQUE_GUID_OFFSET + PARTITION_TYPE_GUID_SIZE],
        )
    );
    crate::log_info!(
        "gpt",
        "Partition entry {}: first_lba={} last_lba={} attributes={:#018x}",
        partition.index,
        partition.first_lba,
        partition.last_lba,
        partition.attributes
    );
    crate::log_debug!(
        "gpt",
        "Partition entry {}: name=\"{}\"",
        partition.index,
        partition.name()
    );
}

impl GuidPartitionTablePartition {
    /// Return the parsed partition name as ASCII fallback text.
    pub fn name(&self) -> &str {
        core::str::from_utf8(&self.name[..self.name_length])
            .expect("GUID partition table partition names are stored as ASCII fallback bytes")
    }
}

struct GuidPartitionTableGuid<'a>(&'a [u8]);

impl fmt::Display for GuidPartitionTableGuid<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let bytes = self.0;
        write!(
            formatter,
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-",
            bytes[3],
            bytes[2],
            bytes[1],
            bytes[0],
            bytes[5],
            bytes[4],
            bytes[7],
            bytes[6],
            bytes[8],
            bytes[9]
        )?;
        for byte in &bytes[10..16] {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

fn parse_partition_name(entry: &[u8]) -> ([u8; PARTITION_NAME_CAPACITY], usize) {
    let mut output = [0; PARTITION_NAME_CAPACITY];
    let mut output_length = 0;
    let name = &entry
        [PARTITION_ENTRY_NAME_OFFSET..PARTITION_ENTRY_NAME_OFFSET + PARTITION_ENTRY_NAME_BYTES];
    for code_unit in name.chunks_exact(2) {
        let value = u16::from_le_bytes([code_unit[0], code_unit[1]]);
        if value == 0 || output_length == output.len() {
            break;
        }

        output[output_length] = if (0x20..=0x7e).contains(&value) {
            u8::try_from(value).expect("ASCII GUID partition table name character must fit in u8")
        } else {
            b'?'
        };
        output_length += 1;
    }

    (output, output_length)
}
