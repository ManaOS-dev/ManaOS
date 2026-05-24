//! File Allocation Table 32 boot sector parsing implementation.

use core::str;

use super::super::block_device::{BlockDevice, SECTOR_BYTES};

const BOOT_SECTOR_LBA: u64 = 0;
const JUMP_INSTRUCTION_OFFSET: usize = 0;
const JUMP_INSTRUCTION_SIZE: usize = 3;
const OEM_NAME_OFFSET: usize = 3;
const OEM_NAME_SIZE: usize = 8;
const BYTES_PER_SECTOR_OFFSET: usize = 11;
const SECTORS_PER_CLUSTER_OFFSET: usize = 13;
const RESERVED_SECTOR_COUNT_OFFSET: usize = 14;
const FILE_ALLOCATION_TABLE_COUNT_OFFSET: usize = 16;
const ROOT_ENTRY_COUNT_OFFSET: usize = 17;
const TOTAL_SECTORS_16_OFFSET: usize = 19;
const MEDIA_DESCRIPTOR_OFFSET: usize = 21;
const FILE_ALLOCATION_TABLE_SIZE_16_OFFSET: usize = 22;
const TOTAL_SECTORS_32_OFFSET: usize = 32;
const FILE_ALLOCATION_TABLE_SIZE_32_OFFSET: usize = 36;
const ROOT_CLUSTER_OFFSET: usize = 44;
const FILE_SYSTEM_INFORMATION_SECTOR_OFFSET: usize = 48;
const BACKUP_BOOT_SECTOR_OFFSET: usize = 50;
const VOLUME_LABEL_OFFSET: usize = 71;
const VOLUME_LABEL_SIZE: usize = 11;
const FILE_SYSTEM_TYPE_OFFSET: usize = 82;
const FILE_SYSTEM_TYPE_SIZE: usize = 8;
const BOOT_SIGNATURE_OFFSET: usize = 510;
const BOOT_SIGNATURE: u16 = 0xaa55;
const SECTOR_BYTES_U16: u16 = 512;
const DIRECTORY_ENTRY_SIZE: usize = 32;
const DIRECTORY_ENTRY_NAME_OFFSET: usize = 0;
const DIRECTORY_ENTRY_NAME_SIZE: usize = 8;
const DIRECTORY_ENTRY_EXTENSION_OFFSET: usize = 8;
const DIRECTORY_ENTRY_EXTENSION_SIZE: usize = 3;
const DIRECTORY_ENTRY_SHORT_NAME_SIZE: usize =
    DIRECTORY_ENTRY_NAME_SIZE + DIRECTORY_ENTRY_EXTENSION_SIZE;
const DIRECTORY_ENTRY_ATTRIBUTE_OFFSET: usize = 11;
const DIRECTORY_ENTRY_FIRST_CLUSTER_HIGH_OFFSET: usize = 20;
const DIRECTORY_ENTRY_FIRST_CLUSTER_LOW_OFFSET: usize = 26;
const DIRECTORY_ENTRY_FILE_SIZE_OFFSET: usize = 28;
const DIRECTORY_ENTRY_END_MARKER: u8 = 0x00;
const DIRECTORY_ENTRY_DELETED_MARKER: u8 = 0xe5;
const DIRECTORY_ENTRY_LONG_NAME_ATTRIBUTE: u8 = 0x0f;
const DIRECTORY_ENTRY_VOLUME_LABEL_ATTRIBUTE: u8 = 0x08;
const DIRECTORY_ENTRY_DIRECTORY_ATTRIBUTE: u8 = 0x10;
const FILE_ALLOCATION_TABLE_ENTRY_BYTES: u32 = 4;
const FILE_ALLOCATION_TABLE_ENTRY_MASK: u32 = 0x0fff_ffff;
const FILE_ALLOCATION_TABLE_END_OF_CHAIN: u32 = 0x0fff_fff8;

/// Parsed File Allocation Table 32 volume geometry.
#[derive(Clone, Copy)]
pub(in crate::kernel::driver::storage) struct FileAllocationTable32Volume {
    /// Number of bytes in one logical sector.
    bytes_per_sector: u16,
    /// Number of sectors in one allocation cluster.
    sectors_per_cluster: u8,
    /// Number of reserved sectors before the file allocation table area.
    reserved_sector_count: u16,
    /// Number of file allocation table copies.
    file_allocation_table_count: u8,
    /// Number of sectors in each file allocation table.
    file_allocation_table_size: u32,
    /// Total number of sectors in the partition.
    total_sectors: u32,
    /// Cluster number of the root directory.
    root_cluster: u32,
    /// First sector of the data region, relative to the partition.
    first_data_sector: u32,
    /// Number of data clusters.
    cluster_count: u32,
}

/// Parsed File Allocation Table 32 directory entry metadata.
#[derive(Clone, Copy)]
pub(in crate::kernel::driver::storage) struct FileAllocationTable32DirectoryEntry {
    short_name: [u8; DIRECTORY_ENTRY_SHORT_NAME_SIZE],
    first_cluster: u32,
    file_size: u32,
}

/// Inspect a partition boot sector as File Allocation Table 32 metadata.
pub(in crate::kernel::driver::storage) fn inspect_boot_sector(
    block_device: &mut impl BlockDevice,
    data_address: u64,
) -> Option<FileAllocationTable32Volume> {
    if !block_device.read_logical_block(BOOT_SECTOR_LBA, data_address) {
        return None;
    }

    let sector = data_address as *const u8;
    // SAFETY: `data_address` points to a 512-byte DMA buffer filled from the
    // selected partition boot sector.
    let sector = unsafe { core::slice::from_raw_parts(sector, SECTOR_BYTES) };

    if read_le_u16(sector, BOOT_SIGNATURE_OFFSET) != BOOT_SIGNATURE {
        crate::log_warn!("fat32", "Boot sector signature not found");
        return None;
    }

    let volume = parse_volume(sector)?;
    log_volume(sector, &volume);

    Some(volume)
}

/// Inspect the File Allocation Table 32 root directory cluster.
pub(in crate::kernel::driver::storage) fn inspect_root_directory(
    block_device: &mut impl BlockDevice,
    volume: FileAllocationTable32Volume,
    data_address: u64,
) -> Option<FileAllocationTable32DirectoryEntry> {
    let root_directory_start_sector = volume.cluster_first_sector(volume.root_cluster)?;
    let mut file_entries = 0;
    let mut first_file = None;

    for sector_offset in 0..volume.sectors_per_cluster {
        let logical_block_address =
            root_directory_start_sector.checked_add(u32::from(sector_offset))?;
        if !block_device.read_logical_block(u64::from(logical_block_address), data_address) {
            return None;
        }

        let sector = data_address as *const u8;
        // SAFETY: `data_address` points to a 512-byte DMA buffer filled from a
        // root-directory sector.
        let sector = unsafe { core::slice::from_raw_parts(sector, SECTOR_BYTES) };
        let reached_end = inspect_directory_sector(sector, &mut file_entries, &mut first_file);
        if reached_end {
            break;
        }
    }

    crate::log_info!(
        "fat32",
        "Root directory: cluster={} entries={}",
        volume.root_cluster,
        file_entries
    );
    first_file
}

/// Inspect the first data cluster for a File Allocation Table 32 file.
pub(in crate::kernel::driver::storage) fn inspect_file_contents(
    block_device: &mut impl BlockDevice,
    volume: FileAllocationTable32Volume,
    entry: FileAllocationTable32DirectoryEntry,
    data_address: u64,
) -> Option<()> {
    if entry.file_size == 0 {
        crate::log_info!("fat32", "Read {}: \"\"", entry.name());
        return Some(());
    }

    let first_sector = volume.cluster_first_sector(entry.first_cluster)?;
    if !block_device.read_logical_block(u64::from(first_sector), data_address) {
        return None;
    }

    let sector = data_address as *const u8;
    // SAFETY: `data_address` points to a 512-byte DMA buffer filled from the
    // file's first data sector.
    let sector = unsafe { core::slice::from_raw_parts(sector, SECTOR_BYTES) };
    let bytes_to_show = usize::try_from(entry.file_size)
        .expect("file size must fit in usize")
        .min(SECTOR_BYTES);
    crate::log_info!(
        "fat32",
        "Read {}: \"{}\"",
        entry.name(),
        EscapedAscii(&sector[..bytes_to_show])
    );

    let next_cluster = read_next_cluster(block_device, &volume, entry.first_cluster, data_address)?;
    if next_cluster >= FILE_ALLOCATION_TABLE_END_OF_CHAIN {
        crate::log_debug!("fat32", "{} cluster chain ends", entry.name());
    } else {
        crate::log_debug!("fat32", "{} next_cluster={}", entry.name(), next_cluster);
    }

    Some(())
}

fn parse_volume(sector: &[u8]) -> Option<FileAllocationTable32Volume> {
    let bytes_per_sector = read_le_u16(sector, BYTES_PER_SECTOR_OFFSET);
    let sectors_per_cluster = sector[SECTORS_PER_CLUSTER_OFFSET];
    let reserved_sector_count = read_le_u16(sector, RESERVED_SECTOR_COUNT_OFFSET);
    let file_allocation_table_count = sector[FILE_ALLOCATION_TABLE_COUNT_OFFSET];
    let root_entry_count = read_le_u16(sector, ROOT_ENTRY_COUNT_OFFSET);
    let total_sectors_16 = read_le_u16(sector, TOTAL_SECTORS_16_OFFSET);
    let total_sectors_32 = read_le_u32(sector, TOTAL_SECTORS_32_OFFSET);
    let file_allocation_table_size_16 = read_le_u16(sector, FILE_ALLOCATION_TABLE_SIZE_16_OFFSET);
    let file_allocation_table_size = read_le_u32(sector, FILE_ALLOCATION_TABLE_SIZE_32_OFFSET);
    let root_cluster = read_le_u32(sector, ROOT_CLUSTER_OFFSET);

    if bytes_per_sector != SECTOR_BYTES_U16 {
        crate::log_warn!("fat32", "Unsupported bytes_per_sector={}", bytes_per_sector);
        return None;
    }
    if sectors_per_cluster == 0 || !sectors_per_cluster.is_power_of_two() {
        crate::log_warn!(
            "fat32",
            "Unsupported sectors_per_cluster={}",
            sectors_per_cluster
        );
        return None;
    }
    if reserved_sector_count == 0 || file_allocation_table_count == 0 {
        crate::log_warn!(
            "fat32",
            "Invalid reserved/FAT counts: reserved={} fats={}",
            reserved_sector_count,
            file_allocation_table_count
        );
        return None;
    }
    if root_entry_count != 0 || total_sectors_16 != 0 || file_allocation_table_size_16 != 0 {
        crate::log_warn!(
            "fat32",
            "Boot sector is not FAT32: root_entries={} total16={} fat16={}",
            root_entry_count,
            total_sectors_16,
            file_allocation_table_size_16
        );
        return None;
    }
    if total_sectors_32 == 0 || file_allocation_table_size == 0 || root_cluster < 2 {
        crate::log_warn!(
            "fat32",
            "Invalid FAT32 geometry: total={} fat_size={} root_cluster={}",
            total_sectors_32,
            file_allocation_table_size,
            root_cluster
        );
        return None;
    }

    let first_data_sector = u32::from(reserved_sector_count).checked_add(
        u32::from(file_allocation_table_count).checked_mul(file_allocation_table_size)?,
    )?;
    if first_data_sector >= total_sectors_32 {
        crate::log_warn!(
            "fat32",
            "FAT32 data region is empty: first_data_sector={} total={}",
            first_data_sector,
            total_sectors_32
        );
        return None;
    }

    let data_sector_count = total_sectors_32 - first_data_sector;
    let cluster_count = data_sector_count / u32::from(sectors_per_cluster);

    Some(FileAllocationTable32Volume {
        bytes_per_sector,
        sectors_per_cluster,
        reserved_sector_count,
        file_allocation_table_count,
        file_allocation_table_size,
        total_sectors: total_sectors_32,
        root_cluster,
        first_data_sector,
        cluster_count,
    })
}

fn log_volume(sector: &[u8], volume: &FileAllocationTable32Volume) {
    crate::log_info!(
        "fat32",
        "Boot sector: bytes_per_sector={} sectors_per_cluster={} reserved={} fats={} fat_size={}",
        volume.bytes_per_sector,
        volume.sectors_per_cluster,
        volume.reserved_sector_count,
        volume.file_allocation_table_count,
        volume.file_allocation_table_size
    );
    crate::log_info!(
        "fat32",
        "Volume: total_sectors={} first_data_sector={} clusters={} root_cluster={}",
        volume.total_sectors,
        volume.first_data_sector,
        volume.cluster_count,
        volume.root_cluster
    );
    crate::log_debug!(
        "fat32",
        "Jump={:02x} {:02x} {:02x} OEM=\"{}\" media={:#04x} fs_info={} backup_boot={} label=\"{}\" type=\"{}\"",
        sector[JUMP_INSTRUCTION_OFFSET],
        sector[JUMP_INSTRUCTION_OFFSET + 1],
        sector[JUMP_INSTRUCTION_OFFSET + JUMP_INSTRUCTION_SIZE - 1],
        ascii_field(&sector[OEM_NAME_OFFSET..OEM_NAME_OFFSET + OEM_NAME_SIZE]),
        sector[MEDIA_DESCRIPTOR_OFFSET],
        read_le_u16(sector, FILE_SYSTEM_INFORMATION_SECTOR_OFFSET),
        read_le_u16(sector, BACKUP_BOOT_SECTOR_OFFSET),
        ascii_field(&sector[VOLUME_LABEL_OFFSET..VOLUME_LABEL_OFFSET + VOLUME_LABEL_SIZE]),
        ascii_field(&sector[FILE_SYSTEM_TYPE_OFFSET..FILE_SYSTEM_TYPE_OFFSET + FILE_SYSTEM_TYPE_SIZE])
    );
}

impl FileAllocationTable32Volume {
    fn cluster_first_sector(self, cluster: u32) -> Option<u32> {
        if cluster < 2 {
            return None;
        }

        let cluster_index = cluster - 2;
        if cluster_index >= self.cluster_count {
            return None;
        }

        self.first_data_sector
            .checked_add(cluster_index.checked_mul(u32::from(self.sectors_per_cluster))?)
    }
}

fn inspect_directory_sector(
    sector: &[u8],
    file_entries: &mut u32,
    first_file: &mut Option<FileAllocationTable32DirectoryEntry>,
) -> bool {
    for entry in sector.chunks_exact(DIRECTORY_ENTRY_SIZE) {
        match entry[0] {
            DIRECTORY_ENTRY_END_MARKER => return true,
            DIRECTORY_ENTRY_DELETED_MARKER => continue,
            _ => {}
        }

        let attribute = entry[DIRECTORY_ENTRY_ATTRIBUTE_OFFSET];
        if attribute == DIRECTORY_ENTRY_LONG_NAME_ATTRIBUTE
            || attribute & DIRECTORY_ENTRY_VOLUME_LABEL_ATTRIBUTE != 0
        {
            continue;
        }

        let directory_entry = FileAllocationTable32DirectoryEntry::new(entry);
        let first_cluster = read_first_cluster(entry);
        let file_size = read_le_u32(entry, DIRECTORY_ENTRY_FILE_SIZE_OFFSET);
        if attribute & DIRECTORY_ENTRY_DIRECTORY_ATTRIBUTE != 0 {
            crate::log_info!(
                "fat32",
                "Directory: {} cluster={}",
                directory_entry.name(),
                first_cluster
            );
        } else {
            *file_entries = file_entries.saturating_add(1);
            if first_file.is_none() {
                *first_file = Some(directory_entry);
            }
            crate::log_info!(
                "fat32",
                "File: {} size={} cluster={}",
                directory_entry.name(),
                file_size,
                first_cluster
            );
        }
    }

    false
}

fn read_next_cluster(
    block_device: &mut impl BlockDevice,
    volume: &FileAllocationTable32Volume,
    cluster: u32,
    data_address: u64,
) -> Option<u32> {
    let byte_offset = cluster.checked_mul(FILE_ALLOCATION_TABLE_ENTRY_BYTES)?;
    let sector_offset = byte_offset / u32::from(volume.bytes_per_sector);
    let entry_offset = usize::try_from(byte_offset % u32::from(volume.bytes_per_sector))
        .expect("file allocation table entry offset must fit in usize");
    let logical_block_address = u64::from(volume.reserved_sector_count) + u64::from(sector_offset);
    if !block_device.read_logical_block(logical_block_address, data_address) {
        return None;
    }

    let sector = data_address as *const u8;
    // SAFETY: `data_address` points to a 512-byte DMA buffer filled from a file
    // allocation table sector.
    let sector = unsafe { core::slice::from_raw_parts(sector, SECTOR_BYTES) };
    Some(read_le_u32(sector, entry_offset) & FILE_ALLOCATION_TABLE_ENTRY_MASK)
}

fn read_first_cluster(entry: &[u8]) -> u32 {
    let high = u32::from(read_le_u16(
        entry,
        DIRECTORY_ENTRY_FIRST_CLUSTER_HIGH_OFFSET,
    ));
    let low = u32::from(read_le_u16(entry, DIRECTORY_ENTRY_FIRST_CLUSTER_LOW_OFFSET));
    (high << 16) | low
}

impl FileAllocationTable32DirectoryEntry {
    fn new(entry: &[u8]) -> Self {
        let mut short_name = [0; DIRECTORY_ENTRY_SHORT_NAME_SIZE];
        short_name.copy_from_slice(
            &entry[DIRECTORY_ENTRY_NAME_OFFSET
                ..DIRECTORY_ENTRY_NAME_OFFSET + DIRECTORY_ENTRY_SHORT_NAME_SIZE],
        );

        Self {
            short_name,
            first_cluster: read_first_cluster(entry),
            file_size: read_le_u32(entry, DIRECTORY_ENTRY_FILE_SIZE_OFFSET),
        }
    }

    fn name(&self) -> ShortFileName<'_> {
        ShortFileName(&self.short_name)
    }
}

struct ShortFileName<'a>(&'a [u8]);

impl core::fmt::Display for ShortFileName<'_> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            formatter,
            "{}",
            ascii_field(
                &self.0[DIRECTORY_ENTRY_NAME_OFFSET
                    ..DIRECTORY_ENTRY_NAME_OFFSET + DIRECTORY_ENTRY_NAME_SIZE]
            )
        )?;

        let extension = ascii_field(
            &self.0[DIRECTORY_ENTRY_EXTENSION_OFFSET
                ..DIRECTORY_ENTRY_EXTENSION_OFFSET + DIRECTORY_ENTRY_EXTENSION_SIZE],
        );
        if !extension.is_empty() {
            write!(formatter, ".{extension}")?;
        }

        Ok(())
    }
}

struct EscapedAscii<'a>(&'a [u8]);

impl core::fmt::Display for EscapedAscii<'_> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for byte in self.0 {
            match *byte {
                b'\r' => write!(formatter, "\\r")?,
                b'\n' => write!(formatter, "\\n")?,
                b'\t' => write!(formatter, "\\t")?,
                0x20..=0x7e => write!(formatter, "{}", char::from(*byte))?,
                _ => write!(formatter, "\\x{byte:02x}")?,
            }
        }
        Ok(())
    }
}

fn read_le_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn read_le_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn ascii_field(bytes: &[u8]) -> &str {
    let trimmed = bytes
        .iter()
        .rposition(|byte| *byte != b' ')
        .map_or(&[][..], |last| &bytes[..=last]);
    str::from_utf8(trimmed).unwrap_or("?")
}
