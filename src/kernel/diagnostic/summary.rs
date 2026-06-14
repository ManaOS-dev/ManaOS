//! Human-readable boot summary output.

/// Status for a boot check displayed in the summary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckStatus {
    /// The check has not been observed.
    Unknown,
    /// The check completed successfully.
    Pass,
    /// The check failed or reported a degraded state.
    Fail,
}

impl CheckStatus {
    /// Return `PASS`, `FAIL`, or `unknown` for summary output.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
        }
    }

    /// Convert a boolean check into a summary status.
    pub const fn from_bool(passed: bool) -> Self {
        if passed {
            Self::Pass
        } else {
            Self::Fail
        }
    }

    const fn is_fail(self) -> bool {
        matches!(self, Self::Fail)
    }

    const fn is_unknown(self) -> bool {
        matches!(self, Self::Unknown)
    }
}

/// Framebuffer mode displayed in the boot summary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FramebufferSummary {
    /// Horizontal resolution in pixels.
    pub width: usize,
    /// Vertical resolution in pixels.
    pub height: usize,
    /// Framebuffer stride in pixels.
    pub stride: usize,
    /// Pixel format label.
    pub format: &'static str,
}

/// Smoke-test status values displayed in the boot summary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SmokeTestSummary {
    /// Filesystem smoke status.
    pub filesystem: CheckStatus,
    /// Syscall smoke status.
    pub syscall: CheckStatus,
    /// Scheduler diagnostics smoke status.
    pub scheduler: CheckStatus,
    /// Private mmap smoke status.
    pub mmap: CheckStatus,
    /// File-backed mmap smoke status.
    pub file_mmap: CheckStatus,
    /// Multi-user preemption smoke status.
    pub preemption: CheckStatus,
}

impl SmokeTestSummary {
    const fn new() -> Self {
        Self {
            filesystem: CheckStatus::Unknown,
            syscall: CheckStatus::Unknown,
            scheduler: CheckStatus::Unknown,
            mmap: CheckStatus::Unknown,
            file_mmap: CheckStatus::Unknown,
            preemption: CheckStatus::Unknown,
        }
    }

    const fn has_failure(self) -> bool {
        self.filesystem.is_fail()
            || self.syscall.is_fail()
            || self.scheduler.is_fail()
            || self.mmap.is_fail()
            || self.file_mmap.is_fail()
            || self.preemption.is_fail()
    }

    const fn has_unknown(self) -> bool {
        self.filesystem.is_unknown()
            || self.syscall.is_unknown()
            || self.scheduler.is_unknown()
            || self.mmap.is_unknown()
            || self.file_mmap.is_unknown()
            || self.preemption.is_unknown()
    }
}

/// Human-readable boot summary state collected by the composition root.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BootSummary {
    /// `ExitBootServices` status.
    pub exit_boot_services: CheckStatus,
    /// Active framebuffer mode.
    pub framebuffer: Option<FramebufferSummary>,
    /// Frame allocator self-check status.
    pub frame_allocator: CheckStatus,
    /// Kernel heap size in MiB.
    pub kernel_heap_mib: Option<usize>,
    /// Free frames observed near the end of boot.
    pub free_frames: Option<u64>,
    /// ACPI root and MADT validation status.
    pub acpi: CheckStatus,
    /// Whether the Local APIC is enabled.
    pub local_apic_enabled: Option<bool>,
    /// Whether IOAPIC routing is active.
    pub ioapic_active: Option<bool>,
    /// Active timer source label.
    pub timer: Option<&'static str>,
    /// APIC EOI count.
    pub apic_eoi_count: Option<u64>,
    /// Legacy PIC EOI count.
    pub legacy_eoi_count: Option<u64>,
    /// AHCI storage status.
    pub ahci: CheckStatus,
    /// GPT parsing status.
    pub gpt: CheckStatus,
    /// FAT32 parsing status.
    pub fat32: CheckStatus,
    /// Number of mounted disk files.
    pub mounted_files: Option<usize>,
    /// Number of spawned user tasks.
    pub user_tasks_spawned: Option<u64>,
    /// Number of exited user tasks.
    pub user_tasks_exited: Option<u64>,
    /// User resource cleanup status.
    pub user_resources_freed: CheckStatus,
    /// Smoke-test statuses.
    pub smoke_tests: SmokeTestSummary,
}

impl BootSummary {
    /// Create an empty boot summary.
    pub const fn new() -> Self {
        Self {
            exit_boot_services: CheckStatus::Unknown,
            framebuffer: None,
            frame_allocator: CheckStatus::Unknown,
            kernel_heap_mib: None,
            free_frames: None,
            acpi: CheckStatus::Unknown,
            local_apic_enabled: None,
            ioapic_active: None,
            timer: None,
            apic_eoi_count: None,
            legacy_eoi_count: None,
            ahci: CheckStatus::Unknown,
            gpt: CheckStatus::Unknown,
            fat32: CheckStatus::Unknown,
            mounted_files: None,
            user_tasks_spawned: None,
            user_tasks_exited: None,
            user_resources_freed: CheckStatus::Unknown,
            smoke_tests: SmokeTestSummary::new(),
        }
    }

    /// Emit the summary to the serial console.
    pub fn emit(&self) {
        crate::serial_println!();
        crate::serial_println!("════════════════ ManaOS Boot Summary ════════════════");
        crate::serial_println!();
        self.emit_boot();
        self.emit_memory();
        self.emit_interrupts();
        self.emit_storage();
        self.emit_userspace();
        self.emit_smoke_tests();
        crate::serial_println!("Status");
        crate::serial_println!("  {}", self.system_status());
        crate::serial_println!();
    }

    fn emit_boot(&self) {
        crate::serial_println!("Boot");
        emit_check("UEFI ExitBootServices", self.exit_boot_services);
        if let Some(framebuffer) = self.framebuffer {
            crate::serial_println!(
                "  Framebuffer           : {}x{} {} stride={}",
                framebuffer.width,
                framebuffer.height,
                framebuffer.format,
                framebuffer.stride
            );
        } else {
            crate::serial_println!("  Framebuffer           : unknown");
        }
        crate::serial_println!();
    }

    fn emit_memory(&self) {
        crate::serial_println!("Memory");
        emit_check("Frame allocator", self.frame_allocator);
        emit_optional_usize("Kernel heap", self.kernel_heap_mib, " MiB");
        emit_optional_u64("Free frames", self.free_frames, "");
        crate::serial_println!();
    }

    fn emit_interrupts(&self) {
        crate::serial_println!("ACPI / Interrupts");
        emit_check("ACPI", self.acpi);
        emit_optional_bool("LAPIC", self.local_apic_enabled, "enabled", "disabled");
        emit_optional_bool("IOAPIC", self.ioapic_active, "active", "inactive");
        emit_optional_str("Timer", self.timer);
        emit_optional_u64("APIC EOI", self.apic_eoi_count, "");
        emit_optional_u64("Legacy EOI", self.legacy_eoi_count, "");
        crate::serial_println!();
    }

    fn emit_storage(&self) {
        crate::serial_println!("Storage");
        emit_check("AHCI", self.ahci);
        emit_check("GPT", self.gpt);
        emit_check("FAT32", self.fat32);
        emit_optional_usize("Mounted files", self.mounted_files, "");
        crate::serial_println!();
    }

    fn emit_userspace(&self) {
        crate::serial_println!("Userspace");
        emit_check("ELF loader", CheckStatus::Pass);
        emit_optional_u64("User tasks spawned", self.user_tasks_spawned, "");
        emit_optional_u64("User tasks exited", self.user_tasks_exited, "");
        emit_check("User resources freed", self.user_resources_freed);
        crate::serial_println!();
    }

    fn emit_smoke_tests(&self) {
        crate::serial_println!("Smoke Tests");
        emit_check("filesystem", self.smoke_tests.filesystem);
        emit_check("syscall", self.smoke_tests.syscall);
        emit_check("scheduler", self.smoke_tests.scheduler);
        emit_check("mmap", self.smoke_tests.mmap);
        emit_check("file mmap", self.smoke_tests.file_mmap);
        emit_check("preemption", self.smoke_tests.preemption);
        crate::serial_println!();
    }

    fn system_status(&self) -> &'static str {
        if self.has_failure() {
            "SYSTEM DEGRADED"
        } else if self.has_unknown() {
            "SYSTEM CHECKS INCOMPLETE"
        } else {
            "SYSTEM HEALTHY"
        }
    }

    fn has_failure(&self) -> bool {
        self.exit_boot_services.is_fail()
            || self.frame_allocator.is_fail()
            || self.acpi.is_fail()
            || self.ahci.is_fail()
            || self.gpt.is_fail()
            || self.fat32.is_fail()
            || self.user_resources_freed.is_fail()
            || self.smoke_tests.has_failure()
    }

    fn has_unknown(&self) -> bool {
        self.exit_boot_services.is_unknown()
            || self.frame_allocator.is_unknown()
            || self.acpi.is_unknown()
            || self.ahci.is_unknown()
            || self.gpt.is_unknown()
            || self.fat32.is_unknown()
            || self.user_resources_freed.is_unknown()
            || self.free_frames.is_none()
            || self.framebuffer.is_none()
            || self.local_apic_enabled.is_none()
            || self.ioapic_active.is_none()
            || self.timer.is_none()
            || self.apic_eoi_count.is_none()
            || self.legacy_eoi_count.is_none()
            || self.mounted_files.is_none()
            || self.user_tasks_spawned.is_none()
            || self.user_tasks_exited.is_none()
            || self.smoke_tests.has_unknown()
    }
}

fn emit_check(label: &str, status: CheckStatus) {
    crate::serial_println!("  {:<22}: {}", label, status.as_str());
}

fn emit_optional_str(label: &str, value: Option<&str>) {
    match value {
        Some(value) => crate::serial_println!("  {:<22}: {}", label, value),
        None => crate::serial_println!("  {:<22}: unknown", label),
    }
}

fn emit_optional_bool(label: &str, value: Option<bool>, true_label: &str, false_label: &str) {
    match value {
        Some(true) => crate::serial_println!("  {:<22}: {}", label, true_label),
        Some(false) => crate::serial_println!("  {:<22}: {}", label, false_label),
        None => crate::serial_println!("  {:<22}: unknown", label),
    }
}

fn emit_optional_usize(label: &str, value: Option<usize>, suffix: &str) {
    match value {
        Some(value) => crate::serial_println!("  {:<22}: {}{}", label, value, suffix),
        None => crate::serial_println!("  {:<22}: unknown", label),
    }
}

fn emit_optional_u64(label: &str, value: Option<u64>, suffix: &str) {
    match value {
        Some(value) => crate::serial_println!("  {:<22}: {}{}", label, value, suffix),
        None => crate::serial_println!("  {:<22}: unknown", label),
    }
}
