//! ACPI root pointer and root table diagnostics types.

mod parsing;

pub use parsing::{inspect_root_pointer, verify_parser_rules};

const ROOT_POINTER_SIGNATURE: &[u8; 8] = b"RSD PTR ";
const ROOT_POINTER_V1_LENGTH: usize = 20;
const ROOT_POINTER_LENGTH_FIELD_END: usize = 24;
const ROOT_POINTER_V2_MIN_LENGTH: usize = 36;
const MAX_ROOT_POINTER_LENGTH: usize = 4096;
const SYSTEM_DESCRIPTION_TABLE_HEADER_LENGTH: usize = 36;
const MAX_SYSTEM_DESCRIPTION_TABLE_LENGTH: usize = 1024 * 1024;
const RSDT_ENTRY_BYTES: usize = 4;
const XSDT_ENTRY_BYTES: usize = 8;
const MADT_SIGNATURE: &[u8; 4] = b"APIC";
const MADT_FIXED_HEADER_LENGTH: usize = SYSTEM_DESCRIPTION_TABLE_HEADER_LENGTH + 8;
const MADT_LOCAL_APIC_ADDRESS_OFFSET: usize = SYSTEM_DESCRIPTION_TABLE_HEADER_LENGTH;
const MADT_FLAGS_OFFSET: usize = SYSTEM_DESCRIPTION_TABLE_HEADER_LENGTH + 4;
const MADT_ENTRY_OFFSET: usize = SYSTEM_DESCRIPTION_TABLE_HEADER_LENGTH + 8;
const MADT_PC_AT_COMPATIBLE_FLAG: u32 = 1;
const MADT_ENTRY_HEADER_LENGTH: usize = 2;
const MADT_LOCAL_APIC_ENTRY_TYPE: u8 = 0;
const MADT_IOAPIC_ENTRY_TYPE: u8 = 1;
const MADT_INTERRUPT_SOURCE_OVERRIDE_ENTRY_TYPE: u8 = 2;
const MADT_LOCAL_APIC_NMI_ENTRY_TYPE: u8 = 4;
const MADT_LOCAL_APIC_ADDRESS_OVERRIDE_ENTRY_TYPE: u8 = 5;
const MADT_X2APIC_ENTRY_TYPE: u8 = 9;
const MADT_LOCAL_APIC_ENTRY_LENGTH: usize = 8;
const MADT_IOAPIC_ENTRY_LENGTH: usize = 12;
const MADT_INTERRUPT_SOURCE_OVERRIDE_ENTRY_LENGTH: usize = 10;
const MADT_LOCAL_APIC_NMI_ENTRY_LENGTH: usize = 6;
const MADT_LOCAL_APIC_ADDRESS_OVERRIDE_ENTRY_LENGTH: usize = 12;
const MADT_X2APIC_ENTRY_LENGTH: usize = 16;
const MAX_MADT_LOCAL_APICS: usize = 32;
const MAX_MADT_IOAPICS: usize = 8;
const MAX_MADT_INTERRUPT_SOURCE_OVERRIDES: usize = 16;
const MAX_MADT_LOCAL_APIC_NMIS: usize = 16;
const MAX_MADT_X2APICS: usize = 32;
const MADT_LOCAL_APIC_ENABLED_FLAG: u32 = 1;
const MADT_LOCAL_APIC_ONLINE_CAPABLE_FLAG: u32 = 2;

/// UEFI configuration table that supplied an ACPI root pointer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RootPointerSource {
    /// The ACPI 1.0 UEFI configuration table.
    UefiAcpi1,
    /// The ACPI 2.0 or newer UEFI configuration table.
    UefiAcpi2,
}

impl RootPointerSource {
    /// Return a stable diagnostics label for this source.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UefiAcpi1 => "uefi_acpi1",
            Self::UefiAcpi2 => "uefi_acpi2",
        }
    }
}

/// Physical location of an ACPI Root System Description Pointer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RootPointer {
    physical_address: u64,
    source: RootPointerSource,
}

impl RootPointer {
    /// Create an ACPI root pointer location from a UEFI configuration table.
    pub const fn new(physical_address: u64, source: RootPointerSource) -> Self {
        Self {
            physical_address,
            source,
        }
    }

    /// Return the physical address of the RSDP.
    pub const fn physical_address(self) -> u64 {
        self.physical_address
    }

    /// Return the UEFI configuration table source.
    pub const fn source(self) -> RootPointerSource {
        self.source
    }
}

/// ACPI root table type used for system description table discovery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RootTableKind {
    /// ACPI 1.0 32-bit Root System Description Table.
    Rsdt,
    /// ACPI 2.0+ 64-bit Extended System Description Table.
    Xsdt,
}

impl RootTableKind {
    const fn signature(self) -> &'static [u8; 4] {
        match self {
            Self::Rsdt => b"RSDT",
            Self::Xsdt => b"XSDT",
        }
    }

    const fn entry_size(self) -> usize {
        match self {
            Self::Rsdt => RSDT_ENTRY_BYTES,
            Self::Xsdt => XSDT_ENTRY_BYTES,
        }
    }

    /// Return a stable diagnostics label for this table kind.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rsdt => "rsdt",
            Self::Xsdt => "xsdt",
        }
    }
}

/// Validated ACPI root-table diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RootTableDiagnostics {
    kind: RootTableKind,
    physical_address: u64,
    length: u32,
    revision: u8,
    entry_count: u64,
}

impl RootTableDiagnostics {
    const fn new(
        kind: RootTableKind,
        physical_address: u64,
        length: u32,
        revision: u8,
        entry_count: u64,
    ) -> Self {
        Self {
            kind,
            physical_address,
            length,
            revision,
            entry_count,
        }
    }

    /// Return whether the root table is an RSDT or XSDT.
    pub const fn kind(self) -> RootTableKind {
        self.kind
    }

    /// Return the physical address of the root table.
    pub const fn physical_address(self) -> u64 {
        self.physical_address
    }

    /// Return the ACPI table length in bytes.
    pub const fn length(self) -> u32 {
        self.length
    }

    /// Return the ACPI table revision byte.
    pub const fn revision(self) -> u8 {
        self.revision
    }

    /// Return the number of SDT pointers stored in the root table.
    pub const fn entry_count(self) -> u64 {
        self.entry_count
    }
}

/// Processor Local APIC information from the MADT.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MadtLocalApic {
    processor_id: u8,
    apic_id: u8,
    flags: u32,
}

impl MadtLocalApic {
    const EMPTY: Self = Self::new(0, 0, 0);

    const fn new(processor_id: u8, apic_id: u8, flags: u32) -> Self {
        Self {
            processor_id,
            apic_id,
            flags,
        }
    }

    /// Return the ACPI processor identifier.
    pub const fn processor_id(self) -> u8 {
        self.processor_id
    }

    /// Return the Local APIC identifier.
    pub const fn apic_id(self) -> u8 {
        self.apic_id
    }

    /// Return the raw Local APIC flags.
    pub const fn flags(self) -> u32 {
        self.flags
    }

    /// Return whether this Local APIC is enabled.
    pub const fn is_enabled(self) -> bool {
        self.flags & MADT_LOCAL_APIC_ENABLED_FLAG != 0
    }

    /// Return whether this Local APIC can be brought online later.
    pub const fn is_online_capable(self) -> bool {
        self.flags & MADT_LOCAL_APIC_ONLINE_CAPABLE_FLAG != 0
    }
}

/// IOAPIC information from the MADT.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MadtIoApic {
    id: u8,
    physical_address: u64,
    global_system_interrupt_base: u32,
}

impl MadtIoApic {
    const EMPTY: Self = Self::new(0, 0, 0);

    const fn new(id: u8, physical_address: u64, global_system_interrupt_base: u32) -> Self {
        Self {
            id,
            physical_address,
            global_system_interrupt_base,
        }
    }

    /// Return the IOAPIC identifier.
    pub const fn id(self) -> u8 {
        self.id
    }

    /// Return the IOAPIC MMIO physical address.
    pub const fn physical_address(self) -> u64 {
        self.physical_address
    }

    /// Return the first global system interrupt handled by this IOAPIC.
    pub const fn global_system_interrupt_base(self) -> u32 {
        self.global_system_interrupt_base
    }
}

/// Legacy IRQ source override information from the MADT.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MadtInterruptSourceOverride {
    bus: u8,
    source_irq: u8,
    global_system_interrupt: u32,
    flags: u16,
}

impl MadtInterruptSourceOverride {
    const EMPTY: Self = Self::new(0, 0, 0, 0);

    const fn new(bus: u8, source_irq: u8, global_system_interrupt: u32, flags: u16) -> Self {
        Self {
            bus,
            source_irq,
            global_system_interrupt,
            flags,
        }
    }

    /// Return the source bus identifier.
    pub const fn bus(self) -> u8 {
        self.bus
    }

    /// Return the legacy IRQ source line.
    pub const fn source_irq(self) -> u8 {
        self.source_irq
    }

    /// Return the global system interrupt used for this source line.
    pub const fn global_system_interrupt(self) -> u32 {
        self.global_system_interrupt
    }

    /// Return the raw polarity and trigger-mode flags.
    pub const fn flags(self) -> u16 {
        self.flags
    }
}

/// Local APIC NMI routing information from the MADT.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MadtLocalApicNmi {
    processor_id: u8,
    flags: u16,
    lint: u8,
}

impl MadtLocalApicNmi {
    const EMPTY: Self = Self::new(0, 0, 0);

    const fn new(processor_id: u8, flags: u16, lint: u8) -> Self {
        Self {
            processor_id,
            flags,
            lint,
        }
    }

    /// Return the ACPI processor identifier.
    pub const fn processor_id(self) -> u8 {
        self.processor_id
    }

    /// Return the raw polarity and trigger-mode flags.
    pub const fn flags(self) -> u16 {
        self.flags
    }

    /// Return the Local APIC LINT input.
    pub const fn lint(self) -> u8 {
        self.lint
    }
}

/// Processor Local x2APIC information from the MADT.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MadtX2Apic {
    x2apic_id: u32,
    flags: u32,
    processor_uid: u32,
}

impl MadtX2Apic {
    const EMPTY: Self = Self::new(0, 0, 0);

    const fn new(x2apic_id: u32, flags: u32, processor_uid: u32) -> Self {
        Self {
            x2apic_id,
            flags,
            processor_uid,
        }
    }

    /// Return the x2APIC identifier.
    pub const fn x2apic_id(self) -> u32 {
        self.x2apic_id
    }

    /// Return the raw x2APIC flags.
    pub const fn flags(self) -> u32 {
        self.flags
    }

    /// Return the ACPI processor UID.
    pub const fn processor_uid(self) -> u32 {
        self.processor_uid
    }

    /// Return whether this x2APIC is enabled.
    pub const fn is_enabled(self) -> bool {
        self.flags & MADT_LOCAL_APIC_ENABLED_FLAG != 0
    }

    /// Return whether this x2APIC can be brought online later.
    pub const fn is_online_capable(self) -> bool {
        self.flags & MADT_LOCAL_APIC_ONLINE_CAPABLE_FLAG != 0
    }
}

/// Bounded MADT interrupt topology records for APIC provider setup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MadtInterruptTopology {
    local_apics: [MadtLocalApic; MAX_MADT_LOCAL_APICS],
    local_apic_records: usize,
    ioapics: [MadtIoApic; MAX_MADT_IOAPICS],
    ioapic_records: usize,
    interrupt_source_overrides: [MadtInterruptSourceOverride; MAX_MADT_INTERRUPT_SOURCE_OVERRIDES],
    interrupt_source_override_records: usize,
    local_apic_nmis: [MadtLocalApicNmi; MAX_MADT_LOCAL_APIC_NMIS],
    local_apic_nmi_records: usize,
    x2apics: [MadtX2Apic; MAX_MADT_X2APICS],
    x2apic_records: usize,
    truncated: bool,
}

impl MadtInterruptTopology {
    const fn new() -> Self {
        Self {
            local_apics: [MadtLocalApic::EMPTY; MAX_MADT_LOCAL_APICS],
            local_apic_records: 0,
            ioapics: [MadtIoApic::EMPTY; MAX_MADT_IOAPICS],
            ioapic_records: 0,
            interrupt_source_overrides: [MadtInterruptSourceOverride::EMPTY;
                MAX_MADT_INTERRUPT_SOURCE_OVERRIDES],
            interrupt_source_override_records: 0,
            local_apic_nmis: [MadtLocalApicNmi::EMPTY; MAX_MADT_LOCAL_APIC_NMIS],
            local_apic_nmi_records: 0,
            x2apics: [MadtX2Apic::EMPTY; MAX_MADT_X2APICS],
            x2apic_records: 0,
            truncated: false,
        }
    }

    /// Return whether any MADT records exceeded the retained topology capacity.
    pub const fn is_truncated(self) -> bool {
        self.truncated
    }

    /// Return the retained Processor Local APIC record count.
    pub const fn retained_local_apic_count(self) -> usize {
        self.local_apic_records
    }

    /// Return the retained IOAPIC record count.
    pub const fn retained_ioapic_count(self) -> usize {
        self.ioapic_records
    }

    /// Return the retained interrupt source override record count.
    pub const fn retained_interrupt_source_override_count(self) -> usize {
        self.interrupt_source_override_records
    }

    /// Return the retained Local APIC NMI record count.
    pub const fn retained_local_apic_nmi_count(self) -> usize {
        self.local_apic_nmi_records
    }

    /// Return the retained Processor Local x2APIC record count.
    pub const fn retained_x2apic_count(self) -> usize {
        self.x2apic_records
    }

    /// Return one retained Processor Local APIC record by index.
    pub const fn local_apic(self, index: usize) -> Option<MadtLocalApic> {
        if index < self.local_apic_records {
            Some(self.local_apics[index])
        } else {
            None
        }
    }

    /// Return one retained IOAPIC record by index.
    pub const fn ioapic(self, index: usize) -> Option<MadtIoApic> {
        if index < self.ioapic_records {
            Some(self.ioapics[index])
        } else {
            None
        }
    }

    /// Return one retained interrupt source override record by index.
    pub const fn interrupt_source_override(
        self,
        index: usize,
    ) -> Option<MadtInterruptSourceOverride> {
        if index < self.interrupt_source_override_records {
            Some(self.interrupt_source_overrides[index])
        } else {
            None
        }
    }

    /// Return one retained Local APIC NMI record by index.
    pub const fn local_apic_nmi(self, index: usize) -> Option<MadtLocalApicNmi> {
        if index < self.local_apic_nmi_records {
            Some(self.local_apic_nmis[index])
        } else {
            None
        }
    }

    /// Return one retained Processor Local x2APIC record by index.
    pub const fn x2apic(self, index: usize) -> Option<MadtX2Apic> {
        if index < self.x2apic_records {
            Some(self.x2apics[index])
        } else {
            None
        }
    }

    /// Return the override for a legacy IRQ source line when one exists.
    pub fn interrupt_source_override_for_legacy_irq(
        self,
        source_irq: u8,
    ) -> Option<MadtInterruptSourceOverride> {
        let mut index = 0;
        while index < self.interrupt_source_override_records {
            let record = self.interrupt_source_overrides[index];
            if record.source_irq() == source_irq {
                return Some(record);
            }
            index += 1;
        }
        None
    }

    /// Return the global system interrupt for a legacy IRQ source line.
    pub fn global_system_interrupt_for_legacy_irq(self, source_irq: u8) -> u32 {
        self.interrupt_source_override_for_legacy_irq(source_irq)
            .map_or(u32::from(source_irq), |source_override| {
                source_override.global_system_interrupt()
            })
    }

    fn push_local_apic(&mut self, record: MadtLocalApic) {
        if self.local_apic_records < MAX_MADT_LOCAL_APICS {
            self.local_apics[self.local_apic_records] = record;
            self.local_apic_records += 1;
        } else {
            self.truncated = true;
        }
    }

    fn push_ioapic(&mut self, record: MadtIoApic) {
        if self.ioapic_records < MAX_MADT_IOAPICS {
            self.ioapics[self.ioapic_records] = record;
            self.ioapic_records += 1;
        } else {
            self.truncated = true;
        }
    }

    fn push_interrupt_source_override(&mut self, record: MadtInterruptSourceOverride) {
        if self.interrupt_source_override_records < MAX_MADT_INTERRUPT_SOURCE_OVERRIDES {
            self.interrupt_source_overrides[self.interrupt_source_override_records] = record;
            self.interrupt_source_override_records += 1;
        } else {
            self.truncated = true;
        }
    }

    fn push_local_apic_nmi(&mut self, record: MadtLocalApicNmi) {
        if self.local_apic_nmi_records < MAX_MADT_LOCAL_APIC_NMIS {
            self.local_apic_nmis[self.local_apic_nmi_records] = record;
            self.local_apic_nmi_records += 1;
        } else {
            self.truncated = true;
        }
    }

    fn push_x2apic(&mut self, record: MadtX2Apic) {
        if self.x2apic_records < MAX_MADT_X2APICS {
            self.x2apics[self.x2apic_records] = record;
            self.x2apic_records += 1;
        } else {
            self.truncated = true;
        }
    }
}

/// Validated ACPI Multiple APIC Description Table diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MadtDiagnostics {
    physical_address: u64,
    length: u32,
    revision: u8,
    local_apic_address: u64,
    flags: u32,
    pc_at_compatible: bool,
    entry_count: u64,
    local_apic_count: u64,
    ioapic_count: u64,
    interrupt_source_override_count: u64,
    local_apic_nmi_count: u64,
    local_apic_address_override_count: u64,
    x2apic_count: u64,
    topology: MadtInterruptTopology,
}

impl MadtDiagnostics {
    /// Return the MADT physical address.
    pub const fn physical_address(self) -> u64 {
        self.physical_address
    }

    /// Return the ACPI table length in bytes.
    pub const fn length(self) -> u32 {
        self.length
    }

    /// Return the ACPI table revision byte.
    pub const fn revision(self) -> u8 {
        self.revision
    }

    /// Return the Local APIC MMIO physical address selected by MADT entries.
    pub const fn local_apic_address(self) -> u64 {
        self.local_apic_address
    }

    /// Return the raw MADT flags field.
    pub const fn flags(self) -> u32 {
        self.flags
    }

    /// Return whether the MADT declares dual 8259 PIC compatibility.
    pub const fn pc_at_compatible(self) -> bool {
        self.pc_at_compatible
    }

    /// Return the number of interrupt controller entries in the MADT.
    pub const fn entry_count(self) -> u64 {
        self.entry_count
    }

    /// Return the number of Processor Local APIC entries.
    pub const fn local_apic_count(self) -> u64 {
        self.local_apic_count
    }

    /// Return the number of IOAPIC entries.
    pub const fn ioapic_count(self) -> u64 {
        self.ioapic_count
    }

    /// Return the number of interrupt source override entries.
    pub const fn interrupt_source_override_count(self) -> u64 {
        self.interrupt_source_override_count
    }

    /// Return the number of Local APIC NMI entries.
    pub const fn local_apic_nmi_count(self) -> u64 {
        self.local_apic_nmi_count
    }

    /// Return the number of Local APIC address override entries.
    pub const fn local_apic_address_override_count(self) -> u64 {
        self.local_apic_address_override_count
    }

    /// Return the number of Processor Local x2APIC entries.
    pub const fn x2apic_count(self) -> u64 {
        self.x2apic_count
    }

    /// Return bounded MADT interrupt topology records.
    pub const fn topology(self) -> MadtInterruptTopology {
        self.topology
    }
}

/// Validated ACPI root pointer and root table diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Diagnostics {
    root_pointer: RootPointer,
    revision: u8,
    rsdt_address: u64,
    xsdt_address: Option<u64>,
    root_table: RootTableDiagnostics,
    madt: MadtDiagnostics,
}

impl Diagnostics {
    /// Return the UEFI-provided RSDP location.
    pub const fn root_pointer(self) -> RootPointer {
        self.root_pointer
    }

    /// Return the ACPI RSDP revision byte.
    pub const fn revision(self) -> u8 {
        self.revision
    }

    /// Return the 32-bit RSDT physical address from the RSDP.
    pub const fn rsdt_address(self) -> u64 {
        self.rsdt_address
    }

    /// Return the 64-bit XSDT physical address when present.
    pub const fn xsdt_address(self) -> Option<u64> {
        self.xsdt_address
    }

    /// Return validated root-table diagnostics.
    pub const fn root_table(self) -> RootTableDiagnostics {
        self.root_table
    }

    /// Return validated MADT diagnostics.
    pub const fn madt(self) -> MadtDiagnostics {
        self.madt
    }
}
