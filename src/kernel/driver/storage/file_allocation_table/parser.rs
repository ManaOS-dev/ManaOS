//! File Allocation Table 32 boot sector parsing implementation.

use alloc::string::String;
use alloc::vec::Vec;

use super::super::block_device::{BlockDevice, SECTOR_BYTES};
use super::bytes::{ascii_field, read_le_u16, read_le_u32};
use crate::kernel::memory::address::StorageDataAddress;

mod directory;

pub(in crate::kernel::driver::storage) use directory::{
    find_entry_by_path, inspect_file_contents, inspect_root_directory, list_root_directory,
    plan_write,
};
pub(in crate::kernel::driver::storage::file_allocation_table) use directory::{
    read_next_cluster, validate_cluster,
};

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
const LONG_FILE_NAME_TEXT_CAPACITY: usize = 128;
const FILE_ALLOCATION_TABLE_ENTRY_BYTES: u32 = 4;
const FILE_ALLOCATION_TABLE_ENTRY_MASK: u32 = 0x0fff_ffff;
const FILE_ALLOCATION_TABLE_BAD_CLUSTER: u32 = 0x0fff_fff7;
const FILE_ALLOCATION_TABLE_END_OF_CHAIN: u32 = 0x0fff_fff8;

/// Return whether a FAT entry marks a bad cluster.
pub(in crate::kernel::driver::storage::file_allocation_table) fn is_bad_cluster(
    cluster: u32,
) -> bool {
    cluster == FILE_ALLOCATION_TABLE_BAD_CLUSTER
}

/// Return whether a FAT entry marks the end of a cluster chain.
pub(in crate::kernel::driver::storage::file_allocation_table) fn is_end_of_chain(
    cluster: u32,
) -> bool {
    cluster >= FILE_ALLOCATION_TABLE_END_OF_CHAIN
}

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
    /// Sector containing File Allocation Table 32 `FSInfo` metadata.
    file_system_information_sector: u16,
    /// Sector containing the backup File Allocation Table 32 boot sector.
    backup_boot_sector: u16,
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
    long_name: [u8; LONG_FILE_NAME_TEXT_CAPACITY],
    long_name_length: usize,
    first_cluster: u32,
    file_size: u32,
    attributes: u8,
}

/// Read-only File Allocation Table 32 directory listing.
pub(in crate::kernel::driver::storage) struct FileAllocationTable32DirectoryListing {
    /// Directory entries found while scanning the directory cluster chain.
    pub entries: Vec<FileAllocationTable32DirectoryEntry>,
}

/// Planned File Allocation Table 32 mutation that has not been executed.
pub(in crate::kernel::driver::storage) struct FileAllocationTable32WritePlan {
    /// Target path for the planned mutation.
    pub path: String,
    /// Number of bytes that would be written.
    pub byte_count: usize,
    /// Number of clusters that would be required.
    pub required_clusters: u32,
}

/// Inspect a partition boot sector as File Allocation Table 32 metadata.
pub(in crate::kernel::driver::storage) fn inspect_boot_sector(
    block_device: &mut impl BlockDevice,
    data_address: StorageDataAddress,
) -> Option<FileAllocationTable32Volume> {
    if let Err(error) = block_device.read_logical_block(BOOT_SECTOR_LBA, data_address) {
        crate::log_warn!("fat32", "Failed to read boot sector: {error:?}");
        return None;
    }

    let sector = data_address.as_usize() as *const u8;
    // SAFETY: `data_address` points to a 512-byte DMA buffer filled from the
    // selected partition boot sector.
    let sector = unsafe { core::slice::from_raw_parts(sector, SECTOR_BYTES) };

    if read_le_u16(sector, BOOT_SIGNATURE_OFFSET) != BOOT_SIGNATURE {
        crate::log_warn!("fat32", "Boot sector signature not found");
        return None;
    }

    let volume = parse_volume(sector)?;
    log_volume(sector, &volume);
    validate_backup_boot_sector(block_device, &volume, sector, data_address);
    super::fsinfo::inspect_file_system_information(block_device, &volume, data_address);

    Some(volume)
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
    let file_system_information_sector = read_le_u16(sector, FILE_SYSTEM_INFORMATION_SECTOR_OFFSET);
    let backup_boot_sector = read_le_u16(sector, BACKUP_BOOT_SECTOR_OFFSET);
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
        file_system_information_sector,
        backup_boot_sector,
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
        volume.file_system_information_sector,
        volume.backup_boot_sector,
        ascii_field(&sector[VOLUME_LABEL_OFFSET..VOLUME_LABEL_OFFSET + VOLUME_LABEL_SIZE]),
        ascii_field(&sector[FILE_SYSTEM_TYPE_OFFSET..FILE_SYSTEM_TYPE_OFFSET + FILE_SYSTEM_TYPE_SIZE])
    );
}

fn validate_backup_boot_sector(
    block_device: &mut impl BlockDevice,
    volume: &FileAllocationTable32Volume,
    primary_sector: &[u8],
    data_address: StorageDataAddress,
) {
    if volume.backup_boot_sector == 0 || volume.backup_boot_sector >= volume.reserved_sector_count {
        crate::log_warn!(
            "fat32",
            "Backup boot sector outside reserved area: backup_boot={} reserved={}",
            volume.backup_boot_sector,
            volume.reserved_sector_count
        );
        return;
    }

    if let Err(error) =
        block_device.read_logical_block(u64::from(volume.backup_boot_sector), data_address)
    {
        crate::log_warn!(
            "fat32",
            "Failed to read backup boot sector: lba={} error={:?}",
            volume.backup_boot_sector,
            error
        );
        return;
    }

    let sector = data_address.as_usize() as *const u8;
    // SAFETY: `data_address` points to a 512-byte DMA buffer filled from the
    // FAT32 backup boot sector.
    let backup_sector = unsafe { core::slice::from_raw_parts(sector, SECTOR_BYTES) };
    if read_le_u16(backup_sector, BOOT_SIGNATURE_OFFSET) != BOOT_SIGNATURE {
        crate::log_warn!("fat32", "Backup boot sector signature not found");
        return;
    }

    let fields_match = boot_sector_field_matches(primary_sector, backup_sector);

    if fields_match {
        crate::log_info!(
            "fat32",
            "Backup boot sector validated: sector={}",
            volume.backup_boot_sector
        );
    } else {
        crate::log_warn!(
            "fat32",
            "Backup boot sector differs from primary metadata: sector={}",
            volume.backup_boot_sector
        );
    }
}

fn boot_sector_field_matches(primary_sector: &[u8], backup_sector: &[u8]) -> bool {
    [
        (BYTES_PER_SECTOR_OFFSET, 2),
        (SECTORS_PER_CLUSTER_OFFSET, 1),
        (RESERVED_SECTOR_COUNT_OFFSET, 2),
        (FILE_ALLOCATION_TABLE_COUNT_OFFSET, 1),
        (TOTAL_SECTORS_32_OFFSET, 4),
        (FILE_ALLOCATION_TABLE_SIZE_32_OFFSET, 4),
        (ROOT_CLUSTER_OFFSET, 4),
        (FILE_SYSTEM_INFORMATION_SECTOR_OFFSET, 2),
        (BACKUP_BOOT_SECTOR_OFFSET, 2),
    ]
    .iter()
    .all(|(offset, size)| {
        primary_sector[*offset..*offset + *size] == backup_sector[*offset..*offset + *size]
    })
}

impl FileAllocationTable32Volume {
    /// Return the first logical sector for a valid data cluster.
    pub(in crate::kernel::driver::storage::file_allocation_table) fn cluster_first_sector(
        self,
        cluster: u32,
    ) -> Option<u32> {
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

    /// Return the byte size of one allocation cluster.
    pub(in crate::kernel::driver::storage::file_allocation_table) fn bytes_per_cluster(
        self,
    ) -> Option<usize> {
        usize::from(self.bytes_per_sector).checked_mul(usize::from(self.sectors_per_cluster))
    }

    /// Return the number of sectors in one allocation cluster.
    pub(in crate::kernel::driver::storage::file_allocation_table) fn sectors_per_cluster(
        self,
    ) -> u8 {
        self.sectors_per_cluster
    }

    /// Return the sector containing `FSInfo` metadata.
    pub(in crate::kernel::driver::storage::file_allocation_table) fn file_system_information_sector(
        self,
    ) -> u16 {
        self.file_system_information_sector
    }

    /// Return the number of reserved sectors before the FAT area.
    pub(in crate::kernel::driver::storage::file_allocation_table) fn reserved_sector_count(
        self,
    ) -> u16 {
        self.reserved_sector_count
    }

    /// Return the number of data clusters in the volume.
    pub(in crate::kernel::driver::storage::file_allocation_table) fn cluster_count(self) -> u32 {
        self.cluster_count
    }
}
