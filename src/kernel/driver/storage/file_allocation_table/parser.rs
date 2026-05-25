//! File Allocation Table 32 boot sector parsing implementation.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::{fmt, str};

use super::super::block_device::{BlockDevice, SECTOR_BYTES};
use super::bytes::{ascii_field, read_le_u16, read_le_u32};
use super::display::EscapedAscii;

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
const FILE_SYSTEM_INFORMATION_LEAD_SIGNATURE_OFFSET: usize = 0;
const FILE_SYSTEM_INFORMATION_STRUCT_SIGNATURE_OFFSET: usize = 484;
const FILE_SYSTEM_INFORMATION_FREE_CLUSTER_COUNT_OFFSET: usize = 488;
const FILE_SYSTEM_INFORMATION_NEXT_FREE_CLUSTER_OFFSET: usize = 492;
const FILE_SYSTEM_INFORMATION_TRAIL_SIGNATURE_OFFSET: usize = 508;
const FILE_SYSTEM_INFORMATION_LEAD_SIGNATURE: u32 = 0x4161_5252;
const FILE_SYSTEM_INFORMATION_STRUCT_SIGNATURE: u32 = 0x6141_7272;
const FILE_SYSTEM_INFORMATION_TRAIL_SIGNATURE: u32 = 0xaa55_0000;
const FILE_SYSTEM_INFORMATION_UNKNOWN: u32 = 0xffff_ffff;
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
    data_address: u64,
) -> Option<FileAllocationTable32Volume> {
    if let Err(error) = block_device.read_logical_block(BOOT_SECTOR_LBA, data_address) {
        crate::log_warn!("fat32", "Failed to read boot sector: {error:?}");
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
    validate_backup_boot_sector(block_device, &volume, sector, data_address);
    inspect_file_system_information(block_device, &volume, data_address);

    Some(volume)
}

/// Inspect the File Allocation Table 32 root directory cluster.
pub(in crate::kernel::driver::storage) fn inspect_root_directory(
    block_device: &mut impl BlockDevice,
    volume: FileAllocationTable32Volume,
    data_address: u64,
) -> Option<FileAllocationTable32DirectoryEntry> {
    let listing = list_directory(block_device, volume, volume.root_cluster, data_address)?;
    let file_entries = listing
        .entries
        .iter()
        .filter(|entry| !entry.is_directory())
        .count();
    let first_file = listing
        .entries
        .iter()
        .find(|entry| !entry.is_directory())
        .copied();

    crate::log_info!(
        "fat32",
        "Root directory: cluster={} entries={}",
        volume.root_cluster,
        file_entries
    );
    first_file
}

/// List the File Allocation Table 32 root directory.
pub(in crate::kernel::driver::storage) fn list_root_directory(
    block_device: &mut impl BlockDevice,
    volume: FileAllocationTable32Volume,
    data_address: u64,
) -> Option<FileAllocationTable32DirectoryListing> {
    list_directory(block_device, volume, volume.root_cluster, data_address)
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
    if let Err(error) = block_device.read_logical_block(u64::from(first_sector), data_address) {
        crate::log_warn!(
            "fat32",
            "Failed to read first data sector lba={}: {error:?}",
            first_sector
        );
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

/// Read the contents of a File Allocation Table 32 regular file.
pub(in crate::kernel::driver::storage) fn read_file_contents(
    block_device: &mut impl BlockDevice,
    volume: FileAllocationTable32Volume,
    entry: FileAllocationTable32DirectoryEntry,
    data_address: u64,
) -> Option<Vec<u8>> {
    let file_size = usize::try_from(entry.file_size).expect("file size must fit in usize");
    let mut contents = Vec::new();
    contents
        .try_reserve_exact(file_size)
        .expect("OOM: failed to reserve FAT32 file contents buffer");

    if file_size == 0 {
        return Some(contents);
    }

    let mut current_cluster = entry.first_cluster;
    let mut visited_clusters = Vec::new();
    let mut clusters_read = 0_u32;
    while contents.len() < file_size {
        if !validate_cluster(&volume, current_cluster, &visited_clusters, entry.name()) {
            return None;
        }
        visited_clusters.push(current_cluster);
        read_cluster_contents(
            block_device,
            &volume,
            current_cluster,
            data_address,
            file_size,
            &mut contents,
        )?;
        clusters_read = clusters_read.saturating_add(1);

        if contents.len() >= file_size {
            break;
        }

        let next_cluster = read_next_cluster(block_device, &volume, current_cluster, data_address)?;
        if next_cluster >= FILE_ALLOCATION_TABLE_END_OF_CHAIN {
            crate::log_error!(
                "fat32",
                "{} ended before file_size={} bytes; read={} bytes",
                entry.name(),
                entry.file_size,
                contents.len()
            );
            return None;
        }
        if next_cluster == FILE_ALLOCATION_TABLE_BAD_CLUSTER
            || volume.cluster_first_sector(next_cluster).is_none()
        {
            crate::log_error!(
                "fat32",
                "{} has invalid next_cluster={:#010x}",
                entry.name(),
                next_cluster
            );
            return None;
        }
        current_cluster = next_cluster;
    }

    crate::log_info!(
        "fat32",
        "Read {} complete: bytes={} clusters_read={}",
        entry.name(),
        contents.len(),
        clusters_read
    );
    Some(contents)
}

/// Plan a read-only File Allocation Table 32 write without mutating disk state.
pub(in crate::kernel::driver::storage) fn plan_write(
    volume: FileAllocationTable32Volume,
    path: &str,
    byte_count: usize,
) -> FileAllocationTable32WritePlan {
    let bytes_per_cluster =
        usize::from(volume.bytes_per_sector) * usize::from(volume.sectors_per_cluster);
    let required_clusters = byte_count.div_ceil(bytes_per_cluster);
    let required_clusters =
        u32::try_from(required_clusters).expect("planned FAT32 cluster count must fit in u32");
    crate::log_info!(
        "fat32",
        "Write plan: path={} bytes={} required_clusters={} mode=read-only",
        path,
        byte_count,
        required_clusters
    );
    FileAllocationTable32WritePlan {
        path: String::from(path),
        byte_count,
        required_clusters,
    }
}

/// Find a directory entry by a slash-separated path relative to the FAT32 root.
pub(in crate::kernel::driver::storage) fn find_entry_by_path(
    block_device: &mut impl BlockDevice,
    volume: FileAllocationTable32Volume,
    path: &str,
    data_address: u64,
) -> Option<FileAllocationTable32DirectoryEntry> {
    let mut current_cluster = volume.root_cluster;
    let mut components = path
        .split('/')
        .filter(|component| !component.is_empty())
        .peekable();

    while let Some(component) = components.next() {
        let listing = list_directory(block_device, volume, current_cluster, data_address)?;
        let entry = listing
            .entries
            .iter()
            .find(|entry| entry.name_matches(component))
            .copied()?;
        if components.peek().is_none() {
            crate::log_info!("fat32", "Path resolved: {} -> {}", path, entry.name());
            return Some(entry);
        }
        if !entry.is_directory() {
            crate::log_warn!("fat32", "Path component is not a directory: {}", component);
            return None;
        }
        current_cluster = entry.first_cluster;
    }

    None
}

impl FileAllocationTable32DirectoryEntry {
    /// Return an absolute lowercase virtual filesystem path under `/disk`.
    pub(in crate::kernel::driver::storage) fn disk_mount_path(&self) -> String {
        let mut mount_path = String::from("/disk/");
        for byte in format!("{}", self.name()).bytes() {
            mount_path.push(char::from(byte.to_ascii_lowercase()));
        }
        mount_path
    }
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
    data_address: u64,
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

    let sector = data_address as *const u8;
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

fn inspect_file_system_information(
    block_device: &mut impl BlockDevice,
    volume: &FileAllocationTable32Volume,
    data_address: u64,
) {
    if volume.file_system_information_sector == 0
        || volume.file_system_information_sector >= volume.reserved_sector_count
    {
        crate::log_warn!(
            "fat32",
            "FSInfo sector outside reserved area: fs_info={} reserved={}",
            volume.file_system_information_sector,
            volume.reserved_sector_count
        );
        return;
    }

    if let Err(error) = block_device.read_logical_block(
        u64::from(volume.file_system_information_sector),
        data_address,
    ) {
        crate::log_warn!(
            "fat32",
            "Failed to read FSInfo sector: lba={} error={:?}",
            volume.file_system_information_sector,
            error
        );
        return;
    }

    let sector = data_address as *const u8;
    // SAFETY: `data_address` points to a 512-byte DMA buffer filled from the
    // FAT32 FSInfo sector.
    let sector = unsafe { core::slice::from_raw_parts(sector, SECTOR_BYTES) };
    let Some(file_system_information) = parse_file_system_information(sector) else {
        return;
    };

    crate::log_info!(
        "fat32",
        "FSInfo: sector={} free_clusters={} next_free_cluster={}",
        volume.file_system_information_sector,
        FileSystemInformationValue(file_system_information.free_cluster_count),
        FileSystemInformationValue(file_system_information.next_free_cluster)
    );

    if file_system_information.free_cluster_count != FILE_SYSTEM_INFORMATION_UNKNOWN
        && file_system_information.free_cluster_count > volume.cluster_count
    {
        crate::log_warn!(
            "fat32",
            "FSInfo free cluster count exceeds volume clusters: free={} clusters={}",
            file_system_information.free_cluster_count,
            volume.cluster_count
        );
    }

    if file_system_information.next_free_cluster != FILE_SYSTEM_INFORMATION_UNKNOWN
        && volume
            .cluster_first_sector(file_system_information.next_free_cluster)
            .is_none()
    {
        crate::log_warn!(
            "fat32",
            "FSInfo next free cluster is outside data area: next_free_cluster={} clusters={}",
            file_system_information.next_free_cluster,
            volume.cluster_count
        );
    }
}

struct FileSystemInformation {
    free_cluster_count: u32,
    next_free_cluster: u32,
}

fn parse_file_system_information(sector: &[u8]) -> Option<FileSystemInformation> {
    let lead_signature = read_le_u32(sector, FILE_SYSTEM_INFORMATION_LEAD_SIGNATURE_OFFSET);
    let struct_signature = read_le_u32(sector, FILE_SYSTEM_INFORMATION_STRUCT_SIGNATURE_OFFSET);
    let trail_signature = read_le_u32(sector, FILE_SYSTEM_INFORMATION_TRAIL_SIGNATURE_OFFSET);

    if lead_signature != FILE_SYSTEM_INFORMATION_LEAD_SIGNATURE
        || struct_signature != FILE_SYSTEM_INFORMATION_STRUCT_SIGNATURE
        || trail_signature != FILE_SYSTEM_INFORMATION_TRAIL_SIGNATURE
    {
        crate::log_warn!(
            "fat32",
            "Invalid FSInfo signatures: lead={:#010x} struct={:#010x} trail={:#010x}",
            lead_signature,
            struct_signature,
            trail_signature
        );
        return None;
    }

    Some(FileSystemInformation {
        free_cluster_count: read_le_u32(sector, FILE_SYSTEM_INFORMATION_FREE_CLUSTER_COUNT_OFFSET),
        next_free_cluster: read_le_u32(sector, FILE_SYSTEM_INFORMATION_NEXT_FREE_CLUSTER_OFFSET),
    })
}

struct FileSystemInformationValue(u32);

impl fmt::Display for FileSystemInformationValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 == FILE_SYSTEM_INFORMATION_UNKNOWN {
            write!(formatter, "unknown")
        } else {
            write!(formatter, "{}", self.0)
        }
    }
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

fn list_directory(
    block_device: &mut impl BlockDevice,
    volume: FileAllocationTable32Volume,
    directory_cluster: u32,
    data_address: u64,
) -> Option<FileAllocationTable32DirectoryListing> {
    let mut listing = FileAllocationTable32DirectoryListing {
        entries: Vec::new(),
    };
    let mut visited_clusters = Vec::new();
    let mut current_cluster = directory_cluster;

    loop {
        if !validate_cluster(&volume, current_cluster, &visited_clusters, "directory") {
            return None;
        }
        visited_clusters.push(current_cluster);

        let first_sector = volume.cluster_first_sector(current_cluster)?;
        for sector_offset in 0..volume.sectors_per_cluster {
            let logical_block_address = first_sector.checked_add(u32::from(sector_offset))?;
            if let Err(error) =
                block_device.read_logical_block(u64::from(logical_block_address), data_address)
            {
                crate::log_warn!(
                    "fat32",
                    "Failed to read directory sector lba={}: {error:?}",
                    logical_block_address
                );
                return None;
            }

            let sector = data_address as *const u8;
            // SAFETY: `data_address` points to a 512-byte DMA buffer filled from
            // a directory sector.
            let sector = unsafe { core::slice::from_raw_parts(sector, SECTOR_BYTES) };
            if inspect_directory_sector(sector, &mut listing) {
                crate::log_info!(
                    "fat32",
                    "Directory listing: cluster={} entries={}",
                    directory_cluster,
                    listing.entries.len()
                );
                return Some(listing);
            }
        }

        let next_cluster = read_next_cluster(block_device, &volume, current_cluster, data_address)?;
        if next_cluster >= FILE_ALLOCATION_TABLE_END_OF_CHAIN {
            break;
        }
        if next_cluster == FILE_ALLOCATION_TABLE_BAD_CLUSTER
            || volume.cluster_first_sector(next_cluster).is_none()
        {
            crate::log_error!(
                "fat32",
                "Directory has invalid next_cluster={:#010x}",
                next_cluster
            );
            return None;
        }
        current_cluster = next_cluster;
    }

    crate::log_info!(
        "fat32",
        "Directory listing: cluster={} entries={} clusters={}",
        directory_cluster,
        listing.entries.len(),
        visited_clusters.len()
    );
    Some(listing)
}

fn inspect_directory_sector(
    sector: &[u8],
    listing: &mut FileAllocationTable32DirectoryListing,
) -> bool {
    let mut long_file_name = LongFileNameBuilder::new();
    for entry in sector.chunks_exact(DIRECTORY_ENTRY_SIZE) {
        match entry[0] {
            DIRECTORY_ENTRY_END_MARKER => return true,
            DIRECTORY_ENTRY_DELETED_MARKER => {
                long_file_name.clear();
                continue;
            }
            _ => {}
        }

        let attribute = entry[DIRECTORY_ENTRY_ATTRIBUTE_OFFSET];
        if attribute == DIRECTORY_ENTRY_LONG_NAME_ATTRIBUTE {
            long_file_name.push(entry);
            continue;
        }
        if attribute & DIRECTORY_ENTRY_VOLUME_LABEL_ATTRIBUTE != 0 {
            long_file_name.clear();
            continue;
        }

        let directory_entry = FileAllocationTable32DirectoryEntry::new(entry, &long_file_name);
        long_file_name.clear();
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
            crate::log_info!(
                "fat32",
                "File: {} size={} cluster={}",
                directory_entry.name(),
                file_size,
                first_cluster
            );
        }
        listing.entries.push(directory_entry);
    }

    false
}

fn validate_cluster(
    volume: &FileAllocationTable32Volume,
    cluster: u32,
    visited_clusters: &[u32],
    context: impl fmt::Display,
) -> bool {
    if cluster == FILE_ALLOCATION_TABLE_BAD_CLUSTER
        || volume.cluster_first_sector(cluster).is_none()
    {
        crate::log_error!("fat32", "{} has invalid cluster={:#010x}", context, cluster);
        return false;
    }

    if visited_clusters.contains(&cluster) {
        crate::log_error!("fat32", "{} has cluster chain loop at {}", context, cluster);
        return false;
    }

    true
}

fn read_cluster_contents(
    block_device: &mut impl BlockDevice,
    volume: &FileAllocationTable32Volume,
    cluster: u32,
    data_address: u64,
    file_size: usize,
    contents: &mut Vec<u8>,
) -> Option<()> {
    let first_sector = volume.cluster_first_sector(cluster)?;
    for sector_offset in 0..volume.sectors_per_cluster {
        let logical_block_address = first_sector.checked_add(u32::from(sector_offset))?;
        if let Err(error) =
            block_device.read_logical_block(u64::from(logical_block_address), data_address)
        {
            crate::log_warn!(
                "fat32",
                "Failed to read file sector lba={}: {error:?}",
                logical_block_address
            );
            return None;
        }

        let sector = data_address as *const u8;
        // SAFETY: `data_address` points to a 512-byte DMA buffer filled from a
        // file data sector.
        let sector = unsafe { core::slice::from_raw_parts(sector, SECTOR_BYTES) };
        let remaining = file_size.saturating_sub(contents.len());
        let bytes_to_copy = remaining.min(SECTOR_BYTES);
        contents.extend_from_slice(&sector[..bytes_to_copy]);
        if contents.len() >= file_size {
            break;
        }
    }

    Some(())
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
    if let Err(error) = block_device.read_logical_block(logical_block_address, data_address) {
        crate::log_warn!(
            "fat32",
            "Failed to read FAT sector lba={}: {error:?}",
            logical_block_address
        );
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
    fn new(entry: &[u8], long_file_name: &LongFileNameBuilder) -> Self {
        let mut short_name = [0; DIRECTORY_ENTRY_SHORT_NAME_SIZE];
        short_name.copy_from_slice(
            &entry[DIRECTORY_ENTRY_NAME_OFFSET
                ..DIRECTORY_ENTRY_NAME_OFFSET + DIRECTORY_ENTRY_SHORT_NAME_SIZE],
        );

        let (long_name, long_name_length) = long_file_name.as_name();
        Self {
            short_name,
            long_name,
            long_name_length,
            first_cluster: read_first_cluster(entry),
            file_size: read_le_u32(entry, DIRECTORY_ENTRY_FILE_SIZE_OFFSET),
            attributes: entry[DIRECTORY_ENTRY_ATTRIBUTE_OFFSET],
        }
    }

    pub(in crate::kernel::driver::storage) fn name(&self) -> DirectoryEntryName<'_> {
        DirectoryEntryName {
            short_name: &self.short_name,
            long_name: if self.long_name_length > 0 {
                Some(&self.long_name[..self.long_name_length])
            } else {
                None
            },
        }
    }

    fn is_directory(&self) -> bool {
        self.attributes & DIRECTORY_ENTRY_DIRECTORY_ATTRIBUTE != 0
    }

    pub(in crate::kernel::driver::storage) fn file_size(&self) -> u32 {
        self.file_size
    }

    fn name_matches(&self, component: &str) -> bool {
        format!("{}", self.name()).eq_ignore_ascii_case(component)
    }
}

struct LongFileNameBuilder {
    text: [u8; LONG_FILE_NAME_TEXT_CAPACITY],
    length: usize,
}

impl LongFileNameBuilder {
    fn new() -> Self {
        Self {
            text: [0; LONG_FILE_NAME_TEXT_CAPACITY],
            length: 0,
        }
    }

    fn clear(&mut self) {
        self.length = 0;
    }

    fn push(&mut self, entry: &[u8]) {
        let mut fragment = [0_u8; 13];
        let mut fragment_length = 0;
        for offset in [1, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30] {
            let code_unit = read_le_u16(entry, offset);
            if code_unit == 0 || code_unit == 0xffff {
                break;
            }
            fragment[fragment_length] = if (0x20..=0x7e).contains(&code_unit) {
                u8::try_from(code_unit).expect("ASCII long file name code unit must fit in u8")
            } else {
                b'?'
            };
            fragment_length += 1;
        }

        self.prepend(&fragment[..fragment_length]);
    }

    fn prepend(&mut self, fragment: &[u8]) {
        if fragment.is_empty() {
            return;
        }

        let fragment_length = fragment
            .len()
            .min(LONG_FILE_NAME_TEXT_CAPACITY.saturating_sub(self.length));
        self.text.copy_within(0..self.length, fragment_length);
        self.text[..fragment_length].copy_from_slice(&fragment[..fragment_length]);
        self.length += fragment_length;
    }

    fn as_name(&self) -> ([u8; LONG_FILE_NAME_TEXT_CAPACITY], usize) {
        (self.text, self.length)
    }
}

/// Display wrapper for a FAT32 short or long directory entry name.
pub(in crate::kernel::driver::storage) struct DirectoryEntryName<'a> {
    short_name: &'a [u8],
    long_name: Option<&'a [u8]>,
}

impl core::fmt::Display for DirectoryEntryName<'_> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(long_name) = self.long_name {
            write!(
                formatter,
                "{}",
                str::from_utf8(long_name).unwrap_or("invalid-long-name")
            )?;
            return Ok(());
        }

        write!(
            formatter,
            "{}",
            ascii_field(
                &self.short_name[DIRECTORY_ENTRY_NAME_OFFSET
                    ..DIRECTORY_ENTRY_NAME_OFFSET + DIRECTORY_ENTRY_NAME_SIZE]
            )
        )?;

        let extension = ascii_field(
            &self.short_name[DIRECTORY_ENTRY_EXTENSION_OFFSET
                ..DIRECTORY_ENTRY_EXTENSION_OFFSET + DIRECTORY_ENTRY_EXTENSION_SIZE],
        );
        if !extension.is_empty() {
            write!(formatter, ".{extension}")?;
        }

        Ok(())
    }
}
