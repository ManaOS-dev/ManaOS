//! Boot-time storage probing through an Advanced Host Controller Interface disk.

use core::fmt;

use crate::kernel::driver::storage::block_device::BlockDevice;
use crate::kernel::driver::storage::{
    file_allocation_table, guid_partition_table, partition::PartitionBlockDevice,
    set_detected_file, set_selected_partition, StorageFile,
};

pub(super) fn inspect_initial_storage(block_device: &mut impl BlockDevice, data_address: u64) {
    if block_device.read_logical_block(0, data_address) {
        dump_sector_prefix("LBA0", data_address);
    }

    if !block_device.read_logical_block(1, data_address) {
        crate::log_warn!("storage", "Failed to read GPT header sector");
        return;
    }

    let Some(header) = guid_partition_table::inspect_header(data_address) else {
        crate::log_warn!("storage", "Failed to parse GPT header");
        return;
    };

    let Some(partition) =
        guid_partition_table::inspect_partition_table(block_device, header, data_address)
    else {
        crate::log_warn!("storage", "Failed to select a GPT partition");
        return;
    };

    crate::log_info!(
        "storage",
        "Selected GPT partition: index={} first_lba={} last_lba={} name=\"{}\"",
        partition.index,
        partition.first_lba,
        partition.last_lba,
        partition.name()
    );
    set_selected_partition(partition);
    let mut partition_device =
        PartitionBlockDevice::new(block_device, partition.first_lba, partition.last_lba);

    let Some(volume) =
        file_allocation_table::inspect_boot_sector(&mut partition_device, data_address)
    else {
        crate::log_warn!("storage", "Failed to parse FAT32 boot sector");
        return;
    };

    let Some(entry) =
        file_allocation_table::inspect_root_directory(&mut partition_device, volume, data_address)
    else {
        crate::log_warn!("storage", "Failed to scan FAT32 root directory");
        return;
    };

    let _ = file_allocation_table::inspect_file_contents(
        &mut partition_device,
        volume,
        entry,
        data_address,
    );
    let Some(contents) = file_allocation_table::read_file_contents(
        &mut partition_device,
        volume,
        entry,
        data_address,
    ) else {
        crate::log_warn!(
            "storage",
            "Failed to load FAT32 file: path={}",
            entry.disk_mount_path()
        );
        return;
    };

    let mount_path = entry.disk_mount_path();
    crate::log_info!(
        "storage",
        "Loaded FAT32 file for virtual filesystem: path={} bytes={}",
        mount_path,
        contents.len()
    );
    set_detected_file(StorageFile {
        mount_path,
        contents,
    });
}

fn dump_sector_prefix(label: &str, data_address: u64) {
    crate::log_debug!("ahci", "{}: {}", label, SectorPrefix { data_address });
}

struct SectorPrefix {
    data_address: u64,
}

impl fmt::Display for SectorPrefix {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let data = self.data_address as *const u8;
        for offset in 0..16 {
            // SAFETY: `data_address` points to a 512-byte DMA read buffer.
            let byte = unsafe { core::ptr::read_volatile(data.add(offset)) };
            write!(formatter, " {byte:02x}")?;
        }
        Ok(())
    }
}
