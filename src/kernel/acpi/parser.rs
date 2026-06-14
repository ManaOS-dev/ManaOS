//! ACPI root pointer and root table parsing.

use core::slice;

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
    const fn new(
        root_pointer: RootPointer,
        parsed_root_pointer: ParsedRootPointer,
        root_table: RootTableDiagnostics,
        madt: &MadtDiagnostics,
    ) -> Self {
        Self {
            root_pointer,
            revision: parsed_root_pointer.revision,
            rsdt_address: parsed_root_pointer.rsdt_address,
            xsdt_address: parsed_root_pointer.xsdt_address,
            root_table,
            madt: *madt,
        }
    }

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ParsedRootPointer {
    revision: u8,
    rsdt_address: u64,
    xsdt_address: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MadtEntryCounts {
    entries: u64,
    local_apics: u64,
    ioapics: u64,
    interrupt_source_overrides: u64,
    local_apic_nmis: u64,
    local_apic_address_overrides: u64,
    x2apics: u64,
}

impl MadtEntryCounts {
    const fn new() -> Self {
        Self {
            entries: 0,
            local_apics: 0,
            ioapics: 0,
            interrupt_source_overrides: 0,
            local_apic_nmis: 0,
            local_apic_address_overrides: 0,
            x2apics: 0,
        }
    }
}

impl ParsedRootPointer {
    const fn root_table_kind(self) -> RootTableKind {
        if self.xsdt_address.is_some() {
            RootTableKind::Xsdt
        } else {
            RootTableKind::Rsdt
        }
    }

    const fn root_table_address(self) -> u64 {
        match self.xsdt_address {
            Some(address) => address,
            None => self.rsdt_address,
        }
    }
}

/// Validate the RSDP and the selected RSDT/XSDT root table.
///
/// Returns `None` when the RSDP signature, table lengths, signatures, or
/// checksums are invalid.
///
/// # Safety
///
/// `root_pointer` must point to firmware-owned ACPI memory that remains mapped
/// in the current page tables. `ManaOS` calls this only after copying the UEFI
/// configuration table address and installing identity mappings for boot
/// memory-map ranges.
pub unsafe fn inspect_root_pointer(root_pointer: RootPointer) -> Option<Diagnostics> {
    let root_pointer_bytes = read_root_pointer_bytes(root_pointer.physical_address())?;
    let parsed_root_pointer = parse_root_pointer_bytes(root_pointer_bytes)?;
    let root_table_address = parsed_root_pointer.root_table_address();
    let root_table_kind = parsed_root_pointer.root_table_kind();
    let root_table_bytes = read_system_description_table_bytes(root_table_address)?;
    let root_table = parse_root_table_bytes(root_table_kind, root_table_address, root_table_bytes)?;
    let madt = inspect_madt(root_table, root_table_bytes)?;
    Some(Diagnostics::new(
        root_pointer,
        parsed_root_pointer,
        root_table,
        &madt,
    ))
}

/// Run parser self-checks over fixed ACPI byte fixtures.
pub fn verify_parser_rules() -> bool {
    let root_pointer = RootPointer::new(0x1000, RootPointerSource::UefiAcpi2);
    let root_pointer_bytes = valid_root_pointer_fixture();
    let root_table_bytes = valid_xsdt_fixture();
    let madt_bytes = valid_madt_fixture();
    let Some(parsed_root_pointer) = parse_root_pointer_bytes(&root_pointer_bytes) else {
        return false;
    };
    let Some(root_table) = parse_root_table_bytes(
        parsed_root_pointer.root_table_kind(),
        parsed_root_pointer.root_table_address(),
        &root_table_bytes,
    ) else {
        return false;
    };
    let Some(madt) = parse_madt_bytes(0x4000, &madt_bytes) else {
        return false;
    };
    let diagnostics = Diagnostics::new(root_pointer, parsed_root_pointer, root_table, &madt);
    let topology = diagnostics.madt().topology();
    let Some(local_apic) = topology.local_apic(0) else {
        return false;
    };
    let Some(ioapic) = topology.ioapic(0) else {
        return false;
    };
    let Some(interrupt_source_override) = topology.interrupt_source_override(0) else {
        return false;
    };
    let Some(local_apic_nmi) = topology.local_apic_nmi(0) else {
        return false;
    };
    diagnostics.revision() == 2
        && diagnostics.rsdt_address() == 0x3000
        && diagnostics.xsdt_address() == Some(0x2000)
        && diagnostics.root_table().kind() == RootTableKind::Xsdt
        && diagnostics.root_table().entry_count() == 1
        && diagnostics.madt().physical_address() == 0x4000
        && diagnostics.madt().local_apic_address() == 0xfee0_1000
        && diagnostics.madt().flags() == MADT_PC_AT_COMPATIBLE_FLAG
        && diagnostics.madt().pc_at_compatible()
        && diagnostics.madt().entry_count() == 5
        && diagnostics.madt().local_apic_count() == 1
        && diagnostics.madt().ioapic_count() == 1
        && diagnostics.madt().interrupt_source_override_count() == 1
        && diagnostics.madt().local_apic_nmi_count() == 1
        && diagnostics.madt().local_apic_address_override_count() == 1
        && diagnostics.madt().x2apic_count() == 0
        && !topology.is_truncated()
        && topology.retained_local_apic_count() == 1
        && topology.retained_ioapic_count() == 1
        && topology.retained_interrupt_source_override_count() == 1
        && topology.retained_local_apic_nmi_count() == 1
        && topology.retained_x2apic_count() == 0
        && local_apic.processor_id() == 0
        && local_apic.apic_id() == 0
        && local_apic.flags() == MADT_LOCAL_APIC_ENABLED_FLAG
        && local_apic.is_enabled()
        && !local_apic.is_online_capable()
        && ioapic.id() == 1
        && ioapic.physical_address() == 0xfec0_0000
        && ioapic.global_system_interrupt_base() == 0
        && interrupt_source_override.bus() == 0
        && interrupt_source_override.source_irq() == 0
        && interrupt_source_override.global_system_interrupt() == 2
        && interrupt_source_override.flags() == 0
        && topology.global_system_interrupt_for_legacy_irq(0) == 2
        && topology.global_system_interrupt_for_legacy_irq(1) == 1
        && local_apic_nmi.processor_id() == 0xff
        && local_apic_nmi.flags() == 0
        && local_apic_nmi.lint() == 1
}

unsafe fn read_root_pointer_bytes(physical_address: u64) -> Option<&'static [u8]> {
    let initial_bytes = mapped_bytes(physical_address, ROOT_POINTER_V1_LENGTH)?;
    if initial_bytes.get(15).copied()? < 2 {
        return Some(initial_bytes);
    }

    let length_bytes = mapped_bytes(physical_address, ROOT_POINTER_LENGTH_FIELD_END)?;
    let length = usize::try_from(read_u32(length_bytes, 20)).ok()?;
    if !(ROOT_POINTER_V2_MIN_LENGTH..=MAX_ROOT_POINTER_LENGTH).contains(&length) {
        return None;
    }

    mapped_bytes(physical_address, length)
}

unsafe fn read_system_description_table_bytes(physical_address: u64) -> Option<&'static [u8]> {
    let header_bytes = mapped_bytes(physical_address, SYSTEM_DESCRIPTION_TABLE_HEADER_LENGTH)?;
    let length = usize::try_from(read_u32(header_bytes, 4)).ok()?;
    if !(SYSTEM_DESCRIPTION_TABLE_HEADER_LENGTH..=MAX_SYSTEM_DESCRIPTION_TABLE_LENGTH)
        .contains(&length)
    {
        return None;
    }

    mapped_bytes(physical_address, length)
}

unsafe fn mapped_bytes(physical_address: u64, byte_len: usize) -> Option<&'static [u8]> {
    if physical_address == 0 || byte_len == 0 {
        return None;
    }

    let virtual_address = usize::try_from(physical_address).ok()?;
    let pointer = virtual_address as *const u8;
    if pointer.is_null() {
        return None;
    }

    // SAFETY: The caller guarantees this physical range is identity mapped and
    // points to firmware-owned ACPI memory of at least `byte_len` bytes.
    Some(unsafe { slice::from_raw_parts(pointer, byte_len) })
}

fn parse_root_pointer_bytes(bytes: &[u8]) -> Option<ParsedRootPointer> {
    if bytes.len() < ROOT_POINTER_V1_LENGTH || &bytes[0..8] != ROOT_POINTER_SIGNATURE {
        return None;
    }
    if !has_valid_checksum(&bytes[..ROOT_POINTER_V1_LENGTH]) {
        return None;
    }

    let revision = bytes[15];
    let rsdt_address = u64::from(read_u32(bytes, 16));
    if revision < 2 {
        return Some(ParsedRootPointer {
            revision,
            rsdt_address,
            xsdt_address: None,
        });
    }

    if bytes.len() < ROOT_POINTER_V2_MIN_LENGTH {
        return None;
    }
    let length = usize::try_from(read_u32(bytes, 20)).ok()?;
    if !(ROOT_POINTER_V2_MIN_LENGTH..=bytes.len()).contains(&length) {
        return None;
    }
    if !has_valid_checksum(&bytes[..length]) {
        return None;
    }

    let xsdt_address = read_u64(bytes, 24);
    Some(ParsedRootPointer {
        revision,
        rsdt_address,
        xsdt_address: (xsdt_address != 0).then_some(xsdt_address),
    })
}

fn parse_root_table_bytes(
    kind: RootTableKind,
    physical_address: u64,
    bytes: &[u8],
) -> Option<RootTableDiagnostics> {
    if bytes.len() < SYSTEM_DESCRIPTION_TABLE_HEADER_LENGTH {
        return None;
    }
    if &bytes[0..4] != kind.signature() {
        return None;
    }
    let length = usize::try_from(read_u32(bytes, 4)).ok()?;
    if !(SYSTEM_DESCRIPTION_TABLE_HEADER_LENGTH..=bytes.len()).contains(&length) {
        return None;
    }
    if !has_valid_checksum(&bytes[..length]) {
        return None;
    }

    let payload_length = length - SYSTEM_DESCRIPTION_TABLE_HEADER_LENGTH;
    let entry_size = kind.entry_size();
    if !payload_length.is_multiple_of(entry_size) {
        return None;
    }
    let entry_count =
        u64::try_from(payload_length / entry_size).expect("ACPI root table entry count fits u64");
    Some(RootTableDiagnostics::new(
        kind,
        physical_address,
        u32::try_from(length).expect("validated ACPI table length fits u32"),
        bytes[8],
        entry_count,
    ))
}

unsafe fn inspect_madt(
    root_table: RootTableDiagnostics,
    root_table_bytes: &[u8],
) -> Option<MadtDiagnostics> {
    let entry_count = usize::try_from(root_table.entry_count()).ok()?;
    for entry_index in 0..entry_count {
        let table_address =
            root_table_entry_address(root_table.kind(), root_table_bytes, entry_index)?;
        if table_address == 0 {
            continue;
        }
        // SAFETY: The caller guarantees ACPI SDT physical ranges referenced by
        // the validated root table are identity mapped firmware-owned memory.
        let Some(table_bytes) = (unsafe { read_system_description_table_bytes(table_address) })
        else {
            continue;
        };
        if table_bytes.len() >= MADT_SIGNATURE.len() && &table_bytes[0..4] == MADT_SIGNATURE {
            return parse_madt_bytes(table_address, table_bytes);
        }
    }

    None
}

fn root_table_entry_address(kind: RootTableKind, bytes: &[u8], entry_index: usize) -> Option<u64> {
    let offset = SYSTEM_DESCRIPTION_TABLE_HEADER_LENGTH
        .checked_add(entry_index.checked_mul(kind.entry_size())?)?;
    match kind {
        RootTableKind::Rsdt => Some(u64::from(read_u32(bytes, offset))),
        RootTableKind::Xsdt => Some(read_u64(bytes, offset)),
    }
}

fn parse_madt_bytes(physical_address: u64, bytes: &[u8]) -> Option<MadtDiagnostics> {
    if bytes.len() < MADT_FIXED_HEADER_LENGTH || &bytes[0..4] != MADT_SIGNATURE {
        return None;
    }
    let length = usize::try_from(read_u32(bytes, 4)).ok()?;
    if !(MADT_FIXED_HEADER_LENGTH..=bytes.len()).contains(&length) {
        return None;
    }
    if !has_valid_checksum(&bytes[..length]) {
        return None;
    }

    let mut local_apic_address = u64::from(read_u32(bytes, MADT_LOCAL_APIC_ADDRESS_OFFSET));
    let flags = read_u32(bytes, MADT_FLAGS_OFFSET);
    let mut counts = MadtEntryCounts::new();
    let mut topology = MadtInterruptTopology::new();
    let mut offset = MADT_ENTRY_OFFSET;
    while offset < length {
        let entry_type = *bytes.get(offset)?;
        let entry_length = usize::from(*bytes.get(offset + 1)?);
        if entry_length < MADT_ENTRY_HEADER_LENGTH {
            return None;
        }
        let next_offset = offset.checked_add(entry_length)?;
        if next_offset > length {
            return None;
        }

        parse_madt_entry(
            bytes,
            offset,
            entry_type,
            entry_length,
            &mut local_apic_address,
            &mut counts,
            &mut topology,
        )?;
        offset = next_offset;
    }

    Some(MadtDiagnostics {
        physical_address,
        length: u32::try_from(length).expect("validated ACPI table length fits u32"),
        revision: bytes[8],
        local_apic_address,
        flags,
        pc_at_compatible: flags & MADT_PC_AT_COMPATIBLE_FLAG != 0,
        entry_count: counts.entries,
        local_apic_count: counts.local_apics,
        ioapic_count: counts.ioapics,
        interrupt_source_override_count: counts.interrupt_source_overrides,
        local_apic_nmi_count: counts.local_apic_nmis,
        local_apic_address_override_count: counts.local_apic_address_overrides,
        x2apic_count: counts.x2apics,
        topology,
    })
}

fn parse_madt_entry(
    bytes: &[u8],
    offset: usize,
    entry_type: u8,
    entry_length: usize,
    local_apic_address: &mut u64,
    counts: &mut MadtEntryCounts,
    topology: &mut MadtInterruptTopology,
) -> Option<()> {
    match entry_type {
        MADT_LOCAL_APIC_ENTRY_TYPE => {
            if entry_length < MADT_LOCAL_APIC_ENTRY_LENGTH {
                return None;
            }
            topology.push_local_apic(MadtLocalApic::new(
                bytes[offset + 2],
                bytes[offset + 3],
                read_u32(bytes, offset + 4),
            ));
            counts.local_apics += 1;
        }
        MADT_IOAPIC_ENTRY_TYPE => {
            if entry_length < MADT_IOAPIC_ENTRY_LENGTH {
                return None;
            }
            topology.push_ioapic(MadtIoApic::new(
                bytes[offset + 2],
                u64::from(read_u32(bytes, offset + 4)),
                read_u32(bytes, offset + 8),
            ));
            counts.ioapics += 1;
        }
        MADT_INTERRUPT_SOURCE_OVERRIDE_ENTRY_TYPE => {
            if entry_length < MADT_INTERRUPT_SOURCE_OVERRIDE_ENTRY_LENGTH {
                return None;
            }
            topology.push_interrupt_source_override(MadtInterruptSourceOverride::new(
                bytes[offset + 2],
                bytes[offset + 3],
                read_u32(bytes, offset + 4),
                read_u16(bytes, offset + 8),
            ));
            counts.interrupt_source_overrides += 1;
        }
        MADT_LOCAL_APIC_NMI_ENTRY_TYPE => {
            if entry_length < MADT_LOCAL_APIC_NMI_ENTRY_LENGTH {
                return None;
            }
            topology.push_local_apic_nmi(MadtLocalApicNmi::new(
                bytes[offset + 2],
                read_u16(bytes, offset + 3),
                bytes[offset + 5],
            ));
            counts.local_apic_nmis += 1;
        }
        MADT_LOCAL_APIC_ADDRESS_OVERRIDE_ENTRY_TYPE => {
            if entry_length < MADT_LOCAL_APIC_ADDRESS_OVERRIDE_ENTRY_LENGTH {
                return None;
            }
            *local_apic_address = read_u64(bytes, offset + 4);
            counts.local_apic_address_overrides += 1;
        }
        MADT_X2APIC_ENTRY_TYPE => {
            if entry_length < MADT_X2APIC_ENTRY_LENGTH {
                return None;
            }
            topology.push_x2apic(MadtX2Apic::new(
                read_u32(bytes, offset + 4),
                read_u32(bytes, offset + 8),
                read_u32(bytes, offset + 12),
            ));
            counts.x2apics += 1;
        }
        _ => {}
    }

    counts.entries += 1;
    Some(())
}

fn valid_root_pointer_fixture() -> [u8; ROOT_POINTER_V2_MIN_LENGTH] {
    let mut bytes = [0_u8; ROOT_POINTER_V2_MIN_LENGTH];
    bytes[0..8].copy_from_slice(ROOT_POINTER_SIGNATURE);
    bytes[9..15].copy_from_slice(b"MANAOS");
    bytes[15] = 2;
    write_u32(&mut bytes, 16, 0x3000);
    write_u32(
        &mut bytes,
        20,
        u32::try_from(ROOT_POINTER_V2_MIN_LENGTH).expect("root pointer fixture length fits u32"),
    );
    write_u64(&mut bytes, 24, 0x2000);
    bytes[8] = checksum_correction(&bytes[..ROOT_POINTER_V1_LENGTH]);
    bytes[32] = checksum_correction(&bytes);
    bytes
}

fn valid_xsdt_fixture() -> [u8; SYSTEM_DESCRIPTION_TABLE_HEADER_LENGTH + XSDT_ENTRY_BYTES] {
    let mut bytes = [0_u8; SYSTEM_DESCRIPTION_TABLE_HEADER_LENGTH + XSDT_ENTRY_BYTES];
    let byte_len = u32::try_from(bytes.len()).expect("XSDT fixture length fits u32");
    bytes[0..4].copy_from_slice(b"XSDT");
    write_u32(&mut bytes, 4, byte_len);
    bytes[8] = 1;
    bytes[10..16].copy_from_slice(b"MANAOS");
    bytes[16..24].copy_from_slice(b"ROOTTEST");
    write_u32(&mut bytes, 24, 1);
    bytes[28..32].copy_from_slice(b"MANA");
    write_u32(&mut bytes, 32, 1);
    write_u64(&mut bytes, SYSTEM_DESCRIPTION_TABLE_HEADER_LENGTH, 0x4000);
    bytes[9] = checksum_correction(&bytes);
    bytes
}

fn valid_madt_fixture() -> [u8; MADT_FIXED_HEADER_LENGTH
       + MADT_LOCAL_APIC_ENTRY_LENGTH
       + MADT_IOAPIC_ENTRY_LENGTH
       + MADT_INTERRUPT_SOURCE_OVERRIDE_ENTRY_LENGTH
       + MADT_LOCAL_APIC_NMI_ENTRY_LENGTH
       + MADT_LOCAL_APIC_ADDRESS_OVERRIDE_ENTRY_LENGTH] {
    let mut bytes = [0_u8;
        MADT_FIXED_HEADER_LENGTH
            + MADT_LOCAL_APIC_ENTRY_LENGTH
            + MADT_IOAPIC_ENTRY_LENGTH
            + MADT_INTERRUPT_SOURCE_OVERRIDE_ENTRY_LENGTH
            + MADT_LOCAL_APIC_NMI_ENTRY_LENGTH
            + MADT_LOCAL_APIC_ADDRESS_OVERRIDE_ENTRY_LENGTH];
    let byte_len = u32::try_from(bytes.len()).expect("MADT fixture length fits u32");
    bytes[0..4].copy_from_slice(MADT_SIGNATURE);
    write_u32(&mut bytes, 4, byte_len);
    bytes[8] = 1;
    bytes[10..16].copy_from_slice(b"MANAOS");
    bytes[16..24].copy_from_slice(b"APICDIAG");
    write_u32(&mut bytes, 24, 1);
    bytes[28..32].copy_from_slice(b"MANA");
    write_u32(&mut bytes, 32, 1);
    write_u32(&mut bytes, MADT_LOCAL_APIC_ADDRESS_OFFSET, 0xfee0_0000);
    write_u32(&mut bytes, MADT_FLAGS_OFFSET, MADT_PC_AT_COMPATIBLE_FLAG);

    let mut offset = MADT_ENTRY_OFFSET;
    bytes[offset] = MADT_LOCAL_APIC_ENTRY_TYPE;
    bytes[offset + 1] = madt_entry_length(MADT_LOCAL_APIC_ENTRY_LENGTH);
    bytes[offset + 2] = 0;
    bytes[offset + 3] = 0;
    write_u32(&mut bytes, offset + 4, 1);

    offset += MADT_LOCAL_APIC_ENTRY_LENGTH;
    bytes[offset] = MADT_IOAPIC_ENTRY_TYPE;
    bytes[offset + 1] = madt_entry_length(MADT_IOAPIC_ENTRY_LENGTH);
    bytes[offset + 2] = 1;
    bytes[offset + 3] = 0;
    write_u32(&mut bytes, offset + 4, 0xfec0_0000);
    write_u32(&mut bytes, offset + 8, 0);

    offset += MADT_IOAPIC_ENTRY_LENGTH;
    bytes[offset] = MADT_INTERRUPT_SOURCE_OVERRIDE_ENTRY_TYPE;
    bytes[offset + 1] = madt_entry_length(MADT_INTERRUPT_SOURCE_OVERRIDE_ENTRY_LENGTH);
    bytes[offset + 2] = 0;
    bytes[offset + 3] = 0;
    write_u32(&mut bytes, offset + 4, 2);
    write_u16(&mut bytes, offset + 8, 0);

    offset += MADT_INTERRUPT_SOURCE_OVERRIDE_ENTRY_LENGTH;
    bytes[offset] = MADT_LOCAL_APIC_NMI_ENTRY_TYPE;
    bytes[offset + 1] = madt_entry_length(MADT_LOCAL_APIC_NMI_ENTRY_LENGTH);
    bytes[offset + 2] = 0xff;
    write_u16(&mut bytes, offset + 3, 0);
    bytes[offset + 5] = 1;

    offset += MADT_LOCAL_APIC_NMI_ENTRY_LENGTH;
    bytes[offset] = MADT_LOCAL_APIC_ADDRESS_OVERRIDE_ENTRY_TYPE;
    bytes[offset + 1] = madt_entry_length(MADT_LOCAL_APIC_ADDRESS_OVERRIDE_ENTRY_LENGTH);
    write_u16(&mut bytes, offset + 2, 0);
    write_u64(&mut bytes, offset + 4, 0xfee0_1000);

    bytes[9] = checksum_correction(&bytes);
    bytes
}

fn has_valid_checksum(bytes: &[u8]) -> bool {
    checksum_sum(bytes) == 0
}

fn checksum_correction(bytes: &[u8]) -> u8 {
    0_u8.wrapping_sub(checksum_sum(bytes))
}

fn checksum_sum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0_u8, |sum, byte| sum.wrapping_add(*byte))
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + core::mem::size_of::<u32>()]
            .try_into()
            .expect("validated ACPI byte slice contains a u32 field"),
    )
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(
        bytes[offset..offset + core::mem::size_of::<u16>()]
            .try_into()
            .expect("validated ACPI byte slice contains a u16 field"),
    )
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(
        bytes[offset..offset + core::mem::size_of::<u64>()]
            .try_into()
            .expect("validated ACPI byte slice contains a u64 field"),
    )
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + core::mem::size_of::<u32>()].copy_from_slice(&value.to_le_bytes());
}

fn write_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + core::mem::size_of::<u64>()].copy_from_slice(&value.to_le_bytes());
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + core::mem::size_of::<u16>()].copy_from_slice(&value.to_le_bytes());
}

fn madt_entry_length(length: usize) -> u8 {
    u8::try_from(length).expect("MADT fixture entry length fits u8")
}
