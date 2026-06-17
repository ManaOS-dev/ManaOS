//! Page-fault diagnostic records shared across architecture and kernel code.

/// Virtual address that triggered a page fault.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct PageFaultAddress(u64);

impl PageFaultAddress {
    /// Create a page-fault address from the architecture CR2 value.
    pub const fn new(address: u64) -> Self {
        Self(address)
    }

    /// Return the raw virtual address for final diagnostics or conversion.
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

/// Raw `x86_64` page-fault error-code bits.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct PageFaultErrorBits(u64);

impl PageFaultErrorBits {
    /// Create an error-code wrapper from the architecture exception frame.
    pub const fn new(bits: u64) -> Self {
        Self(bits)
    }

    /// Return the raw error-code bits for final diagnostics.
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

/// Virtual instruction pointer captured with a page fault.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct PageFaultInstructionPointer(u64);

impl PageFaultInstructionPointer {
    /// Create an instruction-pointer wrapper from the architecture exception frame.
    pub const fn new(address: u64) -> Self {
        Self(address)
    }

    /// Return the raw virtual instruction pointer for final diagnostics.
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

/// Page-fault report passed from architecture exception handling to the kernel.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct PageFaultReport {
    fault_address: PageFaultAddress,
    error_bits: PageFaultErrorBits,
    instruction_pointer: PageFaultInstructionPointer,
}

impl PageFaultReport {
    /// Create a page-fault report from typed architecture boundary values.
    pub const fn new(
        fault_address: PageFaultAddress,
        error_bits: PageFaultErrorBits,
        instruction_pointer: PageFaultInstructionPointer,
    ) -> Self {
        Self {
            fault_address,
            error_bits,
            instruction_pointer,
        }
    }

    /// Return the virtual address that triggered the page fault.
    pub const fn fault_address(self) -> PageFaultAddress {
        self.fault_address
    }

    /// Return the raw `x86_64` page-fault error-code bits.
    pub const fn error_bits(self) -> PageFaultErrorBits {
        self.error_bits
    }

    /// Return the virtual instruction pointer captured with the fault.
    pub const fn instruction_pointer(self) -> PageFaultInstructionPointer {
        self.instruction_pointer
    }
}

/// Verify the page-fault report wrappers preserve their typed fields.
pub fn verify_typed_page_fault_report() -> bool {
    let report = PageFaultReport::new(
        PageFaultAddress::new(0x1000),
        PageFaultErrorBits::new(0b101),
        PageFaultInstructionPointer::new(0x2000),
    );

    report.fault_address().as_u64() == 0x1000
        && report.error_bits().as_u64() == 0b101
        && report.instruction_pointer().as_u64() == 0x2000
}
