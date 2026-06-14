//! FAT32 directory traversal and file entry parsing.

use super::{
    FileAllocationTable32DirectoryEntry, FileAllocationTable32DirectoryListing,
    FileAllocationTable32Volume, FileAllocationTable32WritePlan, DIRECTORY_ENTRY_ATTRIBUTE_OFFSET,
    DIRECTORY_ENTRY_DELETED_MARKER, DIRECTORY_ENTRY_DIRECTORY_ATTRIBUTE,
    DIRECTORY_ENTRY_END_MARKER, DIRECTORY_ENTRY_EXTENSION_OFFSET, DIRECTORY_ENTRY_EXTENSION_SIZE,
    DIRECTORY_ENTRY_FILE_SIZE_OFFSET, DIRECTORY_ENTRY_FIRST_CLUSTER_HIGH_OFFSET,
    DIRECTORY_ENTRY_FIRST_CLUSTER_LOW_OFFSET, DIRECTORY_ENTRY_LONG_NAME_ATTRIBUTE,
    DIRECTORY_ENTRY_NAME_OFFSET, DIRECTORY_ENTRY_NAME_SIZE, DIRECTORY_ENTRY_SHORT_NAME_SIZE,
    DIRECTORY_ENTRY_SIZE, DIRECTORY_ENTRY_VOLUME_LABEL_ATTRIBUTE,
    FILE_ALLOCATION_TABLE_BAD_CLUSTER, FILE_ALLOCATION_TABLE_END_OF_CHAIN,
    FILE_ALLOCATION_TABLE_ENTRY_BYTES, FILE_ALLOCATION_TABLE_ENTRY_MASK,
    LONG_FILE_NAME_TEXT_CAPACITY,
};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::{fmt, str};

use super::super::super::block_device::{BlockDevice, SECTOR_BYTES};
use super::super::bytes::{ascii_field, read_le_u16, read_le_u32};
use super::super::display::EscapedAscii;
use crate::kernel::memory::address::StorageDataAddress;
/// Inspect the File Allocation Table 32 root directory cluster.
pub(in crate::kernel::driver::storage) fn inspect_root_directory(
    block_device: &mut impl BlockDevice,
    volume: FileAllocationTable32Volume,
    data_address: StorageDataAddress,
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
    data_address: StorageDataAddress,
) -> Option<FileAllocationTable32DirectoryListing> {
    list_directory(block_device, volume, volume.root_cluster, data_address)
}

/// Inspect the first data cluster for a File Allocation Table 32 file.
pub(in crate::kernel::driver::storage) fn inspect_file_contents(
    block_device: &mut impl BlockDevice,
    volume: FileAllocationTable32Volume,
    entry: FileAllocationTable32DirectoryEntry,
    data_address: StorageDataAddress,
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

    let sector = data_address.as_usize() as *const u8;
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
#[allow(dead_code)]
pub(in crate::kernel::driver::storage) fn read_file_contents(
    block_device: &mut impl BlockDevice,
    volume: FileAllocationTable32Volume,
    entry: FileAllocationTable32DirectoryEntry,
    data_address: StorageDataAddress,
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
    data_address: StorageDataAddress,
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

fn list_directory(
    block_device: &mut impl BlockDevice,
    volume: FileAllocationTable32Volume,
    directory_cluster: u32,
    data_address: StorageDataAddress,
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

            let sector = data_address.as_usize() as *const u8;
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

/// Validate that a cluster can be read and has not looped.
pub(in crate::kernel::driver::storage::file_allocation_table) fn validate_cluster(
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
    data_address: StorageDataAddress,
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

        let sector = data_address.as_usize() as *const u8;
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

/// Read the FAT entry that follows a cluster in a chain.
pub(in crate::kernel::driver::storage::file_allocation_table) fn read_next_cluster(
    block_device: &mut impl BlockDevice,
    volume: &FileAllocationTable32Volume,
    cluster: u32,
    data_address: StorageDataAddress,
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

    let sector = data_address.as_usize() as *const u8;
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

    /// Return the first cluster for the directory entry.
    pub(in crate::kernel::driver::storage::file_allocation_table) fn first_cluster(&self) -> u32 {
        self.first_cluster
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
