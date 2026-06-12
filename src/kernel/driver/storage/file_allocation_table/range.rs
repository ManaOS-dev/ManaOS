//! Range reads for FAT32 regular files.

use alloc::vec::Vec;

use super::super::block_device::{BlockDevice, SECTOR_BYTES};
use super::parser::{
    is_bad_cluster, is_end_of_chain, read_next_cluster, validate_cluster,
    FileAllocationTable32DirectoryEntry, FileAllocationTable32Volume,
};
use crate::kernel::memory::address::StorageDataAddress;

#[derive(Clone, Copy)]
struct RangeReadWindow {
    cluster_file_position: usize,
    read_start: usize,
    read_end: usize,
}

struct RangeOutput<'a> {
    buffer: &'a mut [u8],
    position: usize,
}

/// Read a byte range from a File Allocation Table 32 regular file.
pub(in crate::kernel::driver::storage) fn read_file_range(
    block_device: &mut impl BlockDevice,
    volume: FileAllocationTable32Volume,
    entry: FileAllocationTable32DirectoryEntry,
    data_address: StorageDataAddress,
    offset: usize,
    output: &mut [u8],
) -> Option<usize> {
    let file_size = usize::try_from(entry.file_size()).expect("file size must fit in usize");
    if output.is_empty() || offset >= file_size {
        return Some(0);
    }

    let bytes_to_read = output.len().min(file_size - offset);
    let end_offset = offset.checked_add(bytes_to_read)?;
    let bytes_per_cluster = volume.bytes_per_cluster()?;
    let mut current_cluster = entry.first_cluster();
    let mut visited_clusters = Vec::new();
    let mut file_position = 0_usize;
    let mut output = RangeOutput {
        buffer: output,
        position: 0,
    };

    while file_position < end_offset {
        if !validate_cluster(&volume, current_cluster, &visited_clusters, entry.name()) {
            return None;
        }
        visited_clusters.push(current_cluster);
        let cluster_end = file_position.checked_add(bytes_per_cluster)?;
        if cluster_end > offset {
            read_cluster_range_contents(
                block_device,
                &volume,
                current_cluster,
                data_address,
                RangeReadWindow {
                    cluster_file_position: file_position,
                    read_start: offset,
                    read_end: end_offset,
                },
                &mut output,
            )?;
        }

        if output.position >= bytes_to_read {
            break;
        }

        let next_cluster = read_next_cluster(block_device, &volume, current_cluster, data_address)?;
        if is_end_of_chain(next_cluster) {
            break;
        }
        if is_bad_cluster(next_cluster) || volume.cluster_first_sector(next_cluster).is_none() {
            crate::log_error!(
                "fat32",
                "{} has invalid next_cluster={:#010x}",
                entry.name(),
                next_cluster
            );
            return None;
        }
        current_cluster = next_cluster;
        file_position = cluster_end;
    }

    crate::log_debug!(
        "fat32",
        "Backend read {}: offset={} bytes={}",
        entry.name(),
        offset,
        output.position
    );
    Some(output.position)
}

fn read_cluster_range_contents(
    block_device: &mut impl BlockDevice,
    volume: &FileAllocationTable32Volume,
    cluster: u32,
    data_address: StorageDataAddress,
    window: RangeReadWindow,
    output: &mut RangeOutput<'_>,
) -> Option<()> {
    let first_sector = volume.cluster_first_sector(cluster)?;
    let mut sector_file_position = window.cluster_file_position;
    for sector_offset in 0..volume.sectors_per_cluster() {
        let sector_end = sector_file_position.checked_add(SECTOR_BYTES)?;
        if sector_end > window.read_start && sector_file_position < window.read_end {
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
            // SAFETY: `data_address` points to a 512-byte DMA buffer filled from
            // a FAT32 file data sector.
            let sector = unsafe { core::slice::from_raw_parts(sector, SECTOR_BYTES) };
            let copy_start = window.read_start.saturating_sub(sector_file_position);
            let copy_end = (window.read_end - sector_file_position).min(SECTOR_BYTES);
            let copy_length = copy_end.saturating_sub(copy_start);
            let output_end = output.position.checked_add(copy_length)?;
            output.buffer[output.position..output_end]
                .copy_from_slice(&sector[copy_start..copy_end]);
            output.position = output_end;
            if output.position >= output.buffer.len() {
                break;
            }
        }

        sector_file_position = sector_end;
    }

    Some(())
}
