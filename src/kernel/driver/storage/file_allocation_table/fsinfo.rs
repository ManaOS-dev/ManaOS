//! File Allocation Table 32 `FSInfo` sector inspection.

use core::fmt;

use super::super::block_device::{BlockDevice, SECTOR_BYTES};
use super::bytes::read_le_u32;
use super::parser::FileAllocationTable32Volume;

const FILE_SYSTEM_INFORMATION_LEAD_SIGNATURE_OFFSET: usize = 0;
const FILE_SYSTEM_INFORMATION_STRUCT_SIGNATURE_OFFSET: usize = 484;
const FILE_SYSTEM_INFORMATION_FREE_CLUSTER_COUNT_OFFSET: usize = 488;
const FILE_SYSTEM_INFORMATION_NEXT_FREE_CLUSTER_OFFSET: usize = 492;
const FILE_SYSTEM_INFORMATION_TRAIL_SIGNATURE_OFFSET: usize = 508;
const FILE_SYSTEM_INFORMATION_LEAD_SIGNATURE: u32 = 0x4161_5252;
const FILE_SYSTEM_INFORMATION_STRUCT_SIGNATURE: u32 = 0x6141_7272;
const FILE_SYSTEM_INFORMATION_TRAIL_SIGNATURE: u32 = 0xaa55_0000;
const FILE_SYSTEM_INFORMATION_UNKNOWN: u32 = 0xffff_ffff;

struct FileSystemInformation {
    free_cluster_count: u32,
    next_free_cluster: u32,
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

pub(super) fn inspect_file_system_information(
    block_device: &mut impl BlockDevice,
    volume: &FileAllocationTable32Volume,
    data_address: u64,
) {
    if volume.file_system_information_sector() == 0
        || volume.file_system_information_sector() >= volume.reserved_sector_count()
    {
        crate::log_warn!(
            "fat32",
            "FSInfo sector outside reserved area: fs_info={} reserved={}",
            volume.file_system_information_sector(),
            volume.reserved_sector_count()
        );
        return;
    }

    if let Err(error) = block_device.read_logical_block(
        u64::from(volume.file_system_information_sector()),
        data_address,
    ) {
        crate::log_warn!(
            "fat32",
            "Failed to read FSInfo sector: lba={} error={:?}",
            volume.file_system_information_sector(),
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
        volume.file_system_information_sector(),
        FileSystemInformationValue(file_system_information.free_cluster_count),
        FileSystemInformationValue(file_system_information.next_free_cluster)
    );

    validate_file_system_information(volume, &file_system_information);
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

fn validate_file_system_information(
    volume: &FileAllocationTable32Volume,
    file_system_information: &FileSystemInformation,
) {
    if file_system_information.free_cluster_count != FILE_SYSTEM_INFORMATION_UNKNOWN
        && file_system_information.free_cluster_count > volume.cluster_count()
    {
        crate::log_warn!(
            "fat32",
            "FSInfo free cluster count exceeds volume clusters: free={} clusters={}",
            file_system_information.free_cluster_count,
            volume.cluster_count()
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
            volume.cluster_count()
        );
    }
}
