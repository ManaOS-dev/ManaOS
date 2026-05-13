use spin::Lazy;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;

pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

static TSS: Lazy<TaskStateSegment> = Lazy::new(|| {
    let mut tss = TaskStateSegment::new();
    tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
        const STACK_SIZE: usize = 4096 * 5;
        static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];

        let stack_start = VirtAddr::from_ptr(&raw const STACK);
        stack_start + STACK_SIZE
    };
    tss
});

struct Selectors {
    code: SegmentSelector,
    data: SegmentSelector,
    tss: SegmentSelector,
}

static GLOBAL_DESCRIPTOR_TABLE: Lazy<(GlobalDescriptorTable, Selectors)> = Lazy::new(|| {
    let mut table = GlobalDescriptorTable::new();
    let code_selector = table.add_entry(Descriptor::kernel_code_segment());
    let data_selector = table.add_entry(Descriptor::kernel_data_segment());
    let tss_selector = table.add_entry(Descriptor::tss_segment(&TSS));
    (
        table,
        Selectors {
            code: code_selector,
            data: data_selector,
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
