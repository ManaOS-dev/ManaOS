use core::convert::TryInto;

const ELF_HEADER_SIZE: usize = 64;
const PROGRAM_HEADER_SIZE: usize = 56;
const ELF_MAGIC: &[u8; 4] = b"\x7fELF";
const ELF_CLASS_64: u8 = 2;
const ELF_DATA_LITTLE_ENDIAN: u8 = 1;
const ELF_VERSION_CURRENT: u32 = 1;
const ELF_TYPE_EXECUTABLE: u16 = 2;
const ELF_MACHINE_X86_64: u16 = 0x3e;
const PT_LOAD: u32 = 1;

/// Validation failure while parsing an ELF64 image.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(super) enum ElfError {
    /// The file is too small to contain an ELF64 header.
    HeaderTooSmall,
    /// The ELF magic bytes are not present.
    BadMagic,
    /// The ELF class is not 64-bit.
    UnsupportedClass,
    /// The ELF byte order is not little-endian.
    UnsupportedEndian,
    /// The ELF version is not the current version.
    UnsupportedVersion,
    /// The ELF type is not an executable.
    UnsupportedType,
    /// The target machine is not `x86_64`.
    UnsupportedMachine,
    /// The program header entry size is not the ELF64 size.
    UnsupportedProgramHeaderSize,
    /// The program header table is empty.
    EmptyProgramHeaderTable,
    /// The program header table is outside the file.
    ProgramHeaderTableOutOfBounds,
    /// A program header entry is outside the file.
    ProgramHeaderOutOfBounds,
}

impl ElfError {
    /// Return a stable diagnostic string for this error.
    pub(super) fn message(self) -> &'static str {
        match self {
            Self::HeaderTooSmall => "ELF header is too small",
            Self::BadMagic => "ELF magic is invalid",
            Self::UnsupportedClass => "ELF class is not 64-bit",
            Self::UnsupportedEndian => "ELF byte order is not little-endian",
            Self::UnsupportedVersion => "ELF version is unsupported",
            Self::UnsupportedType => "ELF type is not executable",
            Self::UnsupportedMachine => "ELF machine is not x86_64",
            Self::UnsupportedProgramHeaderSize => "ELF program header size is unsupported",
            Self::EmptyProgramHeaderTable => "ELF program header table is empty",
            Self::ProgramHeaderTableOutOfBounds => "ELF program header table is out of bounds",
            Self::ProgramHeaderOutOfBounds => "ELF program header entry is out of bounds",
        }
    }
}

/// Parsed ELF64 image with validated header metadata.
pub(super) struct ElfFile<'a> {
    bytes: &'a [u8],
    header: ElfHeader,
}

impl<'a> ElfFile<'a> {
    /// Parse and validate an ELF64 executable image.
    pub(super) fn parse(bytes: &'a [u8]) -> Result<Self, ElfError> {
        if bytes.len() < ELF_HEADER_SIZE {
            return Err(ElfError::HeaderTooSmall);
        }
        if &bytes[0..4] != ELF_MAGIC {
            return Err(ElfError::BadMagic);
        }
        if bytes[4] != ELF_CLASS_64 {
            return Err(ElfError::UnsupportedClass);
        }
        if bytes[5] != ELF_DATA_LITTLE_ENDIAN {
            return Err(ElfError::UnsupportedEndian);
        }
        if u32::from(bytes[6]) != ELF_VERSION_CURRENT {
            return Err(ElfError::UnsupportedVersion);
        }

        let header = ElfHeader {
            elf_type: read_u16(bytes, 16),
            machine: read_u16(bytes, 18),
            version: read_u32(bytes, 20),
            entry: read_u64(bytes, 24),
            program_header_offset: read_u64(bytes, 32),
            header_size: read_u16(bytes, 52),
            program_header_entry_size: read_u16(bytes, 54),
            program_header_count: read_u16(bytes, 56),
        };

        if header.elf_type != ELF_TYPE_EXECUTABLE {
            return Err(ElfError::UnsupportedType);
        }
        if header.machine != ELF_MACHINE_X86_64 {
            return Err(ElfError::UnsupportedMachine);
        }
        if header.version != ELF_VERSION_CURRENT {
            return Err(ElfError::UnsupportedVersion);
        }
        if usize::from(header.header_size) != ELF_HEADER_SIZE {
            return Err(ElfError::HeaderTooSmall);
        }
        if usize::from(header.program_header_entry_size) != PROGRAM_HEADER_SIZE {
            return Err(ElfError::UnsupportedProgramHeaderSize);
        }
        if header.program_header_count == 0 {
            return Err(ElfError::EmptyProgramHeaderTable);
        }

        let table_start = usize::try_from(header.program_header_offset)
            .map_err(|_| ElfError::ProgramHeaderTableOutOfBounds)?;
        let table_size = usize::from(header.program_header_entry_size)
            .checked_mul(usize::from(header.program_header_count))
            .ok_or(ElfError::ProgramHeaderTableOutOfBounds)?;
        let table_end = table_start
            .checked_add(table_size)
            .ok_or(ElfError::ProgramHeaderTableOutOfBounds)?;
        if table_end > bytes.len() {
            return Err(ElfError::ProgramHeaderTableOutOfBounds);
        }

        Ok(Self { bytes, header })
    }

    /// Return the executable entry point.
    pub(super) fn entry(&self) -> u64 {
        self.header.entry
    }

    /// Return the number of program headers in this executable.
    pub(super) fn program_header_count(&self) -> u16 {
        self.header.program_header_count
    }

    /// Return an iterator over parsed program headers.
    pub(super) fn program_headers(&self) -> ProgramHeaderIter<'_> {
        ProgramHeaderIter {
            bytes: self.bytes,
            next_index: 0,
            header: self.header,
        }
    }
}

#[derive(Clone, Copy)]
struct ElfHeader {
    elf_type: u16,
    machine: u16,
    version: u32,
    entry: u64,
    program_header_offset: u64,
    header_size: u16,
    program_header_entry_size: u16,
    program_header_count: u16,
}

/// One ELF64 program header entry.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(super) struct ProgramHeader {
    header_type: u32,
    flags: u32,
    offset: u64,
    virtual_address: u64,
    file_size: u64,
    memory_size: u64,
    alignment: u64,
}

impl ProgramHeader {
    /// Return whether this is a `PT_LOAD` segment.
    pub(super) fn is_load(&self) -> bool {
        self.header_type == PT_LOAD
    }

    /// Return the raw ELF segment flags.
    pub(super) fn flags(&self) -> u32 {
        self.flags
    }

    /// Return the file offset for this segment.
    pub(super) fn offset(&self) -> u64 {
        self.offset
    }

    /// Return the virtual address for this segment.
    pub(super) fn virtual_address(&self) -> u64 {
        self.virtual_address
    }

    /// Return the number of bytes copied from the executable image.
    pub(super) fn file_size(&self) -> u64 {
        self.file_size
    }

    /// Return the number of bytes mapped in memory.
    pub(super) fn memory_size(&self) -> u64 {
        self.memory_size
    }

    /// Return the alignment requested by the executable.
    pub(super) fn alignment(&self) -> u64 {
        self.alignment
    }
}

/// Iterator over ELF64 program headers.
pub(super) struct ProgramHeaderIter<'a> {
    bytes: &'a [u8],
    next_index: u16,
    header: ElfHeader,
}

impl Iterator for ProgramHeaderIter<'_> {
    type Item = Result<ProgramHeader, ElfError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next_index >= self.header.program_header_count {
            return None;
        }

        let table_start = usize::try_from(self.header.program_header_offset).ok()?;
        let entry_offset = usize::from(self.next_index)
            .checked_mul(usize::from(self.header.program_header_entry_size))?;
        let start = table_start.checked_add(entry_offset)?;
        let end = start.checked_add(PROGRAM_HEADER_SIZE)?;
        self.next_index += 1;

        let Some(bytes) = self.bytes.get(start..end) else {
            return Some(Err(ElfError::ProgramHeaderOutOfBounds));
        };

        Some(Ok(ProgramHeader {
            header_type: read_u32(bytes, 0),
            flags: read_u32(bytes, 4),
            offset: read_u64(bytes, 8),
            virtual_address: read_u64(bytes, 16),
            file_size: read_u64(bytes, 32),
            memory_size: read_u64(bytes, 40),
            alignment: read_u64(bytes, 48),
        }))
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(
        bytes[offset..offset + 2]
            .try_into()
            .expect("u16 field in bounds"),
    )
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("u32 field in bounds"),
    )
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .expect("u64 field in bounds"),
    )
}
