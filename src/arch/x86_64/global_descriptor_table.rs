use spin::LazyLock;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::PrivilegeLevel;
use x86_64::VirtAddr;

/// Interrupt stack table index used for double-fault handling.
pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;
/// Ring 3 code segment selector.
pub const USER_CODE_SELECTOR: u16 = 0x1b;
/// Ring 3 data segment selector.
pub const USER_DATA_SELECTOR: u16 = 0x23;

static TSS: LazyLock<TaskStateSegment> = LazyLock::new(|| {
    let mut tss = TaskStateSegment::new();
    tss.privilege_stack_table[0] = {
        const STACK_SIZE: usize = 4096 * 5;
        static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];

        let stack_start = VirtAddr::from_ptr(&raw const STACK);
        stack_start + STACK_SIZE as u64
    };
    tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
        const STACK_SIZE: usize = 4096 * 5;
        static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];

        let stack_start = VirtAddr::from_ptr(&raw const STACK);
        stack_start + STACK_SIZE as u64
    };
    tss
});

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
        let mut user_code_selector = table.append(Descriptor::user_code_segment());
        let mut user_data_selector = table.append(Descriptor::user_data_segment());
        user_code_selector.set_rpl(PrivilegeLevel::Ring3);
        user_data_selector.set_rpl(PrivilegeLevel::Ring3);
        let tss_selector = table.append(Descriptor::tss_segment(&TSS));
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
