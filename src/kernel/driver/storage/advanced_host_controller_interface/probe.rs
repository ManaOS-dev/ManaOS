//! Boot-time storage probing through an Advanced Host Controller Interface disk.

use alloc::format;
use core::fmt;

use crate::kernel::driver::storage::block_device::BlockDevice;
use crate::kernel::driver::storage::{
    file_allocation_table, guid_partition_table, partition::PartitionBlockDevice,
    set_detected_file, set_selected_partition,
};

pub(super) fn inspect_initial_storage(block_device: &mut impl BlockDevice, data_address: u64) {
    if block_device.read_logical_block(0, data_address).is_ok() {
        dump_sector_prefix("LBA0", data_address);
    }

    let Some(header) =
        guid_partition_table::inspect_header_with_fallback(block_device, data_address)
    else {
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
    let _ = file_allocation_table::list_root_directory(&mut partition_device, volume, data_address);
    let entry = file_allocation_table::find_entry_by_path(
        &mut partition_device,
        volume,
        &format!("{}", entry.name()),
        data_address,
    )
    .unwrap_or(entry);
    let write_plan = file_allocation_table::plan_write(
        volume,
        &entry.disk_mount_path(),
        usize::try_from(entry.file_size()).expect("FAT32 file size must fit in usize"),
    );
    crate::log_debug!(
        "storage",
        "FAT32 write plan retained read-only: path={} bytes={} clusters={}",
        write_plan.path,
        write_plan.byte_count,
        write_plan.required_clusters
    );

    let _ = file_allocation_table::inspect_file_contents(
        &mut partition_device,
        volume,
        entry,
        data_address,
    );
    let mount_path = entry.disk_mount_path();
    crate::log_info!(
        "storage",
        "Registered FAT32 file backend for virtual filesystem: path={} bytes={}",
        mount_path,
        entry.file_size()
    );
    set_detected_file(partition, volume, entry);
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
