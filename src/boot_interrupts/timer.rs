//! Local APIC timer boot diagnostics.

use crate::{arch, kernel};

const LOCAL_APIC_TIMER_CALIBRATION_TICKS: u64 = 100;
const LOCAL_APIC_TIMER_POST_ACTIVATION_TICKS: u64 = 5;
const TIMER_SWITCH_SPIN_LIMIT: u64 = 10_000_000;
/// Start masked Local APIC timer calibration.
pub(crate) fn start_local_apic_timer_calibration() {
    let start_ticks = kernel::time::get_timer_ticks();
    // SAFETY: The Local APIC MMIO page was identity-mapped during ACPI
    // verification, and the timer remains masked during this calibration
    // sample.
    let status = unsafe {
        arch::x86_64::interval_timer::start_masked_local_apic_timer_calibration(start_ticks)
    }
    .expect("Local APIC timer calibration requires APIC provider data");
    crate::log_info!(
        "arch",
        "Local APIC timer calibration started: configured={} armed={} masked={} address={:#x} vector={} divide={} lvt_timer={:#x} divide_config={:#x} initial_count={} current_count={} start_ticks={}",
        status.is_configured(),
        status.is_armed(),
        status.is_masked(),
        status.physical_address(),
        status.vector(),
        status.divide_denominator(),
        status.lvt_timer(),
        status.divide_configuration(),
        status.initial_count(),
        status.current_count(),
        status.start_ticks()
    );
}

/// Switch scheduler ticks to the calibrated Local APIC timer.
pub(crate) fn activate_local_apic_timer_ticks() {
    let calibration_ticks = wait_for_timer_ticks(LOCAL_APIC_TIMER_CALIBRATION_TICKS);
    let calibration_status = verify_local_apic_timer_calibration(calibration_ticks);
    arch::x86_64::disable_interrupts();

    // SAFETY: Interrupts are disabled during timer-source switching. IOAPIC
    // routing is active, and the IOAPIC MMIO page was identity-mapped during
    // ACPI verification.
    let timer_route_status = unsafe {
        arch::x86_64::interrupt_controller::mask_ioapic_timer_route_for_local_apic_timer()
    }
    .expect("IOAPIC timer route must be available before Local APIC timer activation");
    assert!(
        timer_route_status.readback_matches() && timer_route_status.is_masked(),
        "IOAPIC timer route must be masked before Local APIC timer activation"
    );
    assert!(
        timer_route_status.is_routing_active(),
        "IOAPIC routing must remain active for keyboard and mouse routes"
    );
    crate::log_info!(
        "arch",
        "IOAPIC timer route masked for Local APIC timer: routing_active={} readback_matches={} masked={} timer_gsi={} table_index={} low_register={:#x} high_register={:#x} low_readback={:#x} high_readback={:#x}",
        timer_route_status.is_routing_active(),
        timer_route_status.readback_matches(),
        timer_route_status.is_masked(),
        timer_route_status.global_system_interrupt(),
        timer_route_status.table_index(),
        timer_route_status.low_register(),
        timer_route_status.high_register(),
        timer_route_status.low_readback(),
        timer_route_status.high_readback()
    );

    // SAFETY: Interrupts are disabled during timer-source switching, and the
    // Local APIC MMIO page remains identity-mapped after ACPI verification.
    let active_status = unsafe {
        arch::x86_64::interval_timer::activate_local_apic_timer_from_calibration(
            calibration_status,
            calibration_ticks,
        )
    };
    assert!(
        active_status.is_configured()
            && active_status.is_running()
            && active_status.is_periodic()
            && !active_status.is_masked(),
        "Local APIC timer must be active, periodic, and unmasked"
    );
    crate::log_info!(
        "arch",
        "Local APIC timer activated: configured={} running={} masked={} periodic={} address={:#x} vector={} divide={} activation_ticks={} current_ticks={} initial_count={} current_count={} calibration_counts_per_tick={} lvt_timer={:#x} divide_config={:#x}",
        active_status.is_configured(),
        active_status.is_running(),
        active_status.is_masked(),
        active_status.is_periodic(),
        active_status.physical_address(),
        active_status.vector(),
        active_status.divide_denominator(),
        active_status.activation_ticks(),
        active_status.current_ticks(),
        active_status.initial_count(),
        active_status.current_count(),
        active_status.calibration_counts_per_tick(),
        active_status.lvt_timer(),
        active_status.divide_configuration()
    );

    arch::x86_64::enable_interrupts();
    verify_local_apic_timer_tick_source(LOCAL_APIC_TIMER_POST_ACTIVATION_TICKS);
}

fn wait_for_timer_ticks(required_ticks: u64) -> u64 {
    let start_ticks = kernel::time::get_timer_ticks();
    let target_ticks = start_ticks
        .checked_add(required_ticks)
        .expect("timer tick wait target overflowed");
    let mut spin_count = 0;
    while kernel::time::get_timer_ticks() < target_ticks && spin_count < TIMER_SWITCH_SPIN_LIMIT {
        x86_64::instructions::hlt();
        spin_count += 1;
    }
    let current_ticks = kernel::time::get_timer_ticks();
    assert!(
        current_ticks >= target_ticks,
        "timer ticks did not advance enough during backend switching"
    );
    current_ticks
}

fn verify_local_apic_timer_calibration(
    current_ticks: u64,
) -> arch::x86_64::interval_timer::LocalApicTimerCalibrationStatus {
    // SAFETY: The Local APIC MMIO page remains identity-mapped for the kernel
    // after boot-time APIC setup.
    let status = unsafe {
        arch::x86_64::interval_timer::inspect_masked_local_apic_timer_calibration(current_ticks)
    }
    .expect("Local APIC timer calibration sample must be armed before verification");
    assert!(
        status.is_configured() && status.is_armed(),
        "Local APIC timer calibration sample must stay configured and armed"
    );
    assert!(
        status.is_masked(),
        "Local APIC timer calibration must not unmask the timer interrupt"
    );
    assert!(
        status.elapsed_ticks() > 0,
        "PIT ticks must advance before Local APIC timer calibration verification"
    );
    assert!(
        status.has_decremented(),
        "Local APIC timer current count must decrease during calibration"
    );
    assert!(
        !status.has_expired(),
        "Local APIC timer calibration sample must not expire before verification"
    );
    assert!(
        status.counts_per_tick() > 0,
        "Local APIC timer calibration must observe counts per PIT tick"
    );
    crate::log_info!(
        "arch",
        "Local APIC timer calibration verified: configured={} armed={} masked={} decremented={} expired={} address={:#x} vector={} divide={} start_ticks={} current_ticks={} elapsed_ticks={} initial_count={} current_count={} elapsed_counts={} counts_per_tick={} lvt_timer={:#x} divide_config={:#x}",
        status.is_configured(),
        status.is_armed(),
        status.is_masked(),
        status.has_decremented(),
        status.has_expired(),
        status.physical_address(),
        status.vector(),
        status.divide_denominator(),
        status.start_ticks(),
        status.current_ticks(),
        status.elapsed_ticks(),
        status.initial_count(),
        status.current_count(),
        status.elapsed_counts(),
        status.counts_per_tick(),
        status.lvt_timer(),
        status.divide_configuration()
    );
    status
}

fn verify_local_apic_timer_tick_source(required_ticks: u64) {
    let current_ticks = wait_for_timer_ticks(required_ticks);
    let status = verify_active_local_apic_timer(current_ticks);
    log_active_local_apic_timer_status("Local APIC timer tick source verified", status);
}

/// Verify the active Local APIC timer after userspace smoke tests.
pub(crate) fn verify_local_apic_timer_post_smoke() -> bool {
    let status = verify_active_local_apic_timer(kernel::time::get_timer_ticks());
    log_active_local_apic_timer_status("Local APIC timer post-smoke verified", status);
    true
}

fn verify_active_local_apic_timer(
    current_ticks: u64,
) -> arch::x86_64::interval_timer::LocalApicTimerActiveStatus {
    // SAFETY: The Local APIC MMIO page remains identity-mapped for the kernel
    // after boot-time APIC setup.
    let status =
        unsafe { arch::x86_64::interval_timer::inspect_active_local_apic_timer(current_ticks) }
            .expect("Local APIC timer must be active before inspection");
    assert!(
        status.is_configured()
            && status.is_running()
            && status.is_periodic()
            && !status.is_masked(),
        "Local APIC timer must remain active, periodic, and unmasked"
    );
    assert!(
        status.elapsed_ticks() > 0,
        "Local APIC timer must advance scheduler ticks after activation"
    );
    status
}

fn log_active_local_apic_timer_status(
    message: &str,
    status: arch::x86_64::interval_timer::LocalApicTimerActiveStatus,
) {
    crate::log_info!(
        "arch",
        "{}: configured={} running={} masked={} periodic={} address={:#x} vector={} divide={} activation_ticks={} current_ticks={} elapsed_ticks={} initial_count={} current_count={} calibration_counts_per_tick={} lvt_timer={:#x} divide_config={:#x}",
        message,
        status.is_configured(),
        status.is_running(),
        status.is_masked(),
        status.is_periodic(),
        status.physical_address(),
        status.vector(),
        status.divide_denominator(),
        status.activation_ticks(),
        status.current_ticks(),
        status.elapsed_ticks(),
        status.initial_count(),
        status.current_count(),
        status.calibration_counts_per_tick(),
        status.lvt_timer(),
        status.divide_configuration()
    );
}
