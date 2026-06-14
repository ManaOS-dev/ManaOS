use core::cell::UnsafeCell;

use spin::LazyLock;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::PrivilegeLevel;
use x86_64::VirtAddr;

/// Interrupt stack table index used for double-fault handling.
pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;
/// Ring 3 data segment selector.
pub const USER_DATA_SELECTOR: u16 = 0x1b;
/// Ring 3 code segment selector.
pub const USER_CODE_SELECTOR: u16 = 0x23;

const DEFAULT_STACK_SIZE: usize = 4096 * 5;

static TSS: LazyLock<MutableTaskStateSegment> = LazyLock::new(MutableTaskStateSegment::new);
static DEFAULT_PRIVILEGE_STACK: BootStack = BootStack::new();
static DEFAULT_DOUBLE_FAULT_STACK: BootStack = BootStack::new();

struct BootStack {
    bytes: UnsafeCell<[u8; DEFAULT_STACK_SIZE]>,
}

// SAFETY: ManaOS currently runs on one CPU during GDT/TSS setup, and each
// BootStack has a single architecture-owned stack role.
unsafe impl Sync for BootStack {}

impl BootStack {
    #[allow(clippy::large_stack_arrays)]
    const fn new() -> Self {
        Self {
            bytes: UnsafeCell::new([0; DEFAULT_STACK_SIZE]),
        }
    }

    fn top(&'static self) -> VirtAddr {
        let stack_start = VirtAddr::from_ptr(self.bytes.get().cast_const().cast::<u8>());
        stack_start + DEFAULT_STACK_SIZE as u64
    }
}

struct MutableTaskStateSegment {
    segment: UnsafeCell<TaskStateSegment>,
}

// SAFETY: ManaOS currently runs on one CPU. TSS updates are requested by the
// scheduler before entering user mode, and the GDT initialization only reads
// the stable TSS address.
unsafe impl Sync for MutableTaskStateSegment {}

impl MutableTaskStateSegment {
    fn new() -> Self {
        let mut segment = TaskStateSegment::new();
        segment.privilege_stack_table[0] = default_privilege_stack_top();
        segment.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] =
            default_double_fault_stack_top();
        Self {
            segment: UnsafeCell::new(segment),
        }
    }

    fn as_ref(&self) -> &TaskStateSegment {
        // SAFETY: The TSS storage has a stable address for the lifetime of the
        // kernel. Shared reads are used for GDT descriptor creation.
        unsafe { &*self.segment.get() }
    }

    fn set_privilege_stack_top(&self, stack_top: VirtAddr) {
        // SAFETY: The scheduler requests updates before Ring 3 entry on the
        // single active CPU, so no concurrent CPU can read a partially updated
        // stack pointer.
        unsafe {
            (*self.segment.get()).privilege_stack_table[0] = stack_top;
        }
    }
}

fn default_privilege_stack_top() -> VirtAddr {
    DEFAULT_PRIVILEGE_STACK.top()
}

fn default_double_fault_stack_top() -> VirtAddr {
    DEFAULT_DOUBLE_FAULT_STACK.top()
}

struct Selectors {
    code: SegmentSelector,
    data: SegmentSelector,
    user_code: SegmentSelector,
    user_data: SegmentSelector,
    tss: SegmentSelector,
}

static GLOBAL_DESCRIPTOR_TABLE: LazyLock<(GlobalDescriptorTable, Selectors)> =
    LazyLock::new(|| {
        let mut table = GlobalDescriptorTable::new();
        let code_selector = table.append(Descriptor::kernel_code_segment());
        let data_selector = table.append(Descriptor::kernel_data_segment());
        let mut user_data_selector = table.append(Descriptor::user_data_segment());
        let mut user_code_selector = table.append(Descriptor::user_code_segment());
        user_data_selector.set_rpl(PrivilegeLevel::Ring3);
        user_code_selector.set_rpl(PrivilegeLevel::Ring3);
        let tss_selector = table.append(Descriptor::tss_segment(TSS.as_ref()));
        (
            table,
            Selectors {
                code: code_selector,
                data: data_selector,
                user_code: user_code_selector,
                user_data: user_data_selector,
                tss: tss_selector,
            },
        )
    });

pub fn init() {
    use x86_64::instructions::segmentation::{Segment, CS, DS, ES, SS};
    use x86_64::instructions::tables::load_tss;

    GLOBAL_DESCRIPTOR_TABLE.0.load();
    // SAFETY: Selectors come from the loaded global descriptor table and the task
    // state segment descriptor is initialized above.
    unsafe {
        CS::set_reg(GLOBAL_DESCRIPTOR_TABLE.1.code);
        DS::set_reg(GLOBAL_DESCRIPTOR_TABLE.1.data);
        ES::set_reg(GLOBAL_DESCRIPTOR_TABLE.1.data);
        SS::set_reg(GLOBAL_DESCRIPTOR_TABLE.1.data);
        load_tss(GLOBAL_DESCRIPTOR_TABLE.1.tss);
    }
}

/// Install the Ring 0 stack used by the next Ring 3 privilege transition.
pub fn set_privilege_stack_top(stack_top: u64) {
    TSS.set_privilege_stack_top(VirtAddr::new(stack_top));
}

/// Return the user data segment selector.
#[allow(dead_code)]
pub fn get_user_data_selector() -> SegmentSelector {
    GLOBAL_DESCRIPTOR_TABLE.1.user_data
}

/// Return the user code segment selector.
#[allow(dead_code)]
pub fn get_user_code_selector() -> SegmentSelector {
    GLOBAL_DESCRIPTOR_TABLE.1.user_code
}
