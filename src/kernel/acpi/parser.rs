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

/// Validated ACPI root pointer and root table diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Diagnostics {
    root_pointer: RootPointer,
    revision: u8,
    rsdt_address: u64,
    xsdt_address: Option<u64>,
    root_table: RootTableDiagnostics,
}

impl Diagnostics {
    const fn new(
        root_pointer: RootPointer,
        parsed_root_pointer: ParsedRootPointer,
        root_table: RootTableDiagnostics,
    ) -> Self {
        Self {
            root_pointer,
            revision: parsed_root_pointer.revision,
            rsdt_address: parsed_root_pointer.rsdt_address,
            xsdt_address: parsed_root_pointer.xsdt_address,
            root_table,
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ParsedRootPointer {
    revision: u8,
    rsdt_address: u64,
    xsdt_address: Option<u64>,
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
    Some(Diagnostics::new(
        root_pointer,
        parsed_root_pointer,
        root_table,
    ))
}

/// Run parser self-checks over fixed ACPI byte fixtures.
pub fn verify_parser_rules() -> bool {
    let root_pointer = RootPointer::new(0x1000, RootPointerSource::UefiAcpi2);
    let root_pointer_bytes = valid_root_pointer_fixture();
    let root_table_bytes = valid_xsdt_fixture();
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
    let diagnostics = Diagnostics::new(root_pointer, parsed_root_pointer, root_table);
    diagnostics.revision() == 2
        && diagnostics.rsdt_address() == 0x3000
        && diagnostics.xsdt_address() == Some(0x2000)
        && diagnostics.root_table().kind() == RootTableKind::Xsdt
        && diagnostics.root_table().entry_count() == 1
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
