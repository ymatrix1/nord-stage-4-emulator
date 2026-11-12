//! Scenario tests of the interrupt controller: dispatch through the vector table, masking,
//! priority, nesting, the status stack, `IDLE` and the firmware's wake-up path.
use super::interrupts::{IRPTEN, NESTM, VECTOR_PARCELS};
use super::support::load;
use super::vectors::{slot, Slot};
use super::Core;

/// Table base for the tests: away from the helper programs at `0`.
const IVT: u32 = 0x4000;

/// Writes ISA word `w` (a 48-bit instruction as printed by ADI, e.g. `0x063e_0000_0100`) as
/// word `k` of vector `v`: parcels least significant first.
fn vector_word(c: &mut Core, v: u32, k: u32, w: u64) {
    let at = IVT + VECTOR_PARCELS * v + 3 * k;
    load(c, at, &[w as u16, (w >> 16) as u16, (w >> 32) as u16]);
}

/// A core running NOPs at 0 with the vector `v` set to `JUMP isr` (word 0) and an `RTI` at
/// `isr`, interrupts globally enabled and `v` unmasked.
fn core_with_isr(v: u8, isr: u32) -> Core {
    let mut c = Core::new(&[]);
    c.set_ivt_base(IVT);
    vector_word(&mut c, v as u32, 0, 0x063e_0000_0000 | u64::from(isr));
    load(&mut c, 0, &[0x0001; 64]);
    load(&mut c, isr, &[0x0bbe]);
    c.mode1 |= IRPTEN;
    if let Slot::Irptl(b) = slot(v) {
        c.imask |= 1 << b;
    }
    c
}

#[test]
fn taken_interrupt_runs_its_vector_and_returns_after_rti() {
    let mut c = core_with_isr(11, 0x100);
    c.pc = 0x10;
    assert!(c.raise_interrupt(11));
    assert_eq!(c.step(), Ok(())); // dispatch, then the JUMP in the vector
    assert_eq!(c.pc, 0x100, "the vector jump leads to the service routine");
    assert_eq!(c.servicing(), Some(11));
    assert_eq!(c.imaskp_value(), 1 << 11);
    assert_eq!(c.irq.irptl & (1 << 11), 0, "the latch is cleared on entry");
    assert_eq!(c.return_addresses(), &[0x10]);
    assert_eq!(c.step(), Ok(())); // RTI
    assert_eq!((c.pc, c.servicing(), c.imaskp_value()), (0x10, None, 0));
    assert_eq!(c.interrupts_taken(), 1);
    assert!(c.cycles() > c.steps, "entry latency is counted in cycles");
}

#[test]
fn masked_or_globally_disabled_interrupts_stay_latched() {
    let mut c = core_with_isr(11, 0x100);
    c.imask = 0;
    c.raise_interrupt(11);
    assert_eq!(c.step(), Ok(()));
    assert_eq!(
        (c.pc, c.irq.irptl >> 11 & 1),
        (1, 1),
        "masked: latched, not taken"
    );
    c.imask = 1 << 11;
    c.mode1 &= !IRPTEN;
    c.poke_irq();
    assert_eq!(c.step(), Ok(()));
    assert_eq!(c.pc, 2, "IRPTEN clear: not taken");
    c.mode1 |= IRPTEN;
    c.poke_irq();
    assert_eq!(c.step(), Ok(()));
    assert_eq!(
        c.pc, 0x100,
        "unmasking and enabling takes the pending interrupt"
    );
}

#[test]
fn lower_vector_number_wins_and_equal_sources_wait() {
    let mut c = core_with_isr(15, 0x100);
    vector_word(&mut c, 11, 0, 0x063e_0000_0200);
    c.imask |= 1 << 11;
    c.raise_interrupt(15);
    c.raise_interrupt(11);
    assert_eq!(c.step(), Ok(()));
    assert_eq!(c.pc, 0x200, "P0I (vector 11) beats P4I (vector 15)");
    assert_eq!(c.irq.irptl >> 15 & 1, 1, "P4I stays latched");
}

#[test]
fn without_nesting_a_higher_priority_interrupt_waits_for_rti() {
    let mut c = core_with_isr(15, 0x100);
    vector_word(&mut c, 11, 0, 0x063e_0000_0200);
    c.imask |= 1 << 11;
    load(&mut c, 0x200, &[0x0bbe]);
    c.raise_interrupt(15);
    assert_eq!(c.step(), Ok(()));
    assert_eq!(c.pc, 0x100);
    c.raise_interrupt(11);
    assert_eq!(c.step(), Ok(())); // the RTI of P4I
    assert_eq!(c.step(), Ok(())); // P0I is taken now
    assert_eq!(
        c.servicing(),
        Some(11),
        "taken only after the first ISR returned"
    );
}

#[test]
fn with_nesting_only_a_higher_priority_interrupt_preempts() {
    let mut c = core_with_isr(15, 0x100);
    vector_word(&mut c, 11, 0, 0x063e_0000_0200);
    vector_word(&mut c, 20, 0, 0x063e_0000_0300);
    c.imask |= 1 << 11;
    c.irq.lirptl |= 1 << 13; // P9I (vector 20) unmasked
    c.mode1 |= NESTM;
    load(&mut c, 0x100, &[0x0001, 0x0001, 0x0bbe]);
    c.raise_interrupt(15);
    assert_eq!(c.step(), Ok(()));
    c.raise_interrupt(11);
    c.raise_interrupt(20);
    assert_eq!(c.step(), Ok(()));
    assert_eq!(c.servicing(), Some(11), "P0I nests in P4I");
    assert_eq!(c.imaskp_value(), 1 << 11 | 1 << 15);
    // P9I (lower priority than the running P0I) is still waiting
    assert_eq!(c.irq.lirptl >> 3 & 1, 1);
}

#[test]
fn interrupts_wait_for_the_delay_slots_of_a_branch() {
    let mut c = core_with_isr(11, 0x100);
    // JUMP 0x40 (DB) at 0: the two slots at 3 and 4 run before the jump
    load(&mut c, 0, &[0x063e, 0x0400, 0x0040, 0x0001, 0x0001]);
    assert_eq!(c.step(), Ok(())); // the branch is issued
    c.raise_interrupt(11);
    assert_eq!(c.step(), Ok(())); // first slot: the interrupt must wait
    assert_eq!(c.pc, 4);
    assert_eq!(c.step(), Ok(()));
    assert_eq!(c.pc, 0x40, "branch completed first");
    assert_eq!(c.step(), Ok(()));
    assert_eq!(c.pc, 0x100, "then the interrupt is taken");
    assert_eq!(c.return_addresses(), &[0x40]);
}

#[test]
fn running_isr_clears_its_own_latch_and_blocks_reentry() {
    let mut c = core_with_isr(11, 0x100);
    c.raise_interrupt(11);
    assert_eq!(c.step(), Ok(()));
    assert!(!c.raise_interrupt(11), "no self nesting");
    c.write_irptl(1 << 11);
    assert_eq!(c.irq.irptl >> 11 & 1, 0);
}

#[test]
fn irptl_is_software_writable_except_emui_and_rsti() {
    let mut c = Core::new(&[]);
    c.write_ureg(0x7A, 0xFFFF_FFFF);
    assert_eq!(c.read_ureg(0x7A), 0xFFFF_FFFC);
    c.write_ureg(0x7A, 0);
    assert_eq!(c.read_ureg(0x7A), 0);
    // a software interrupt through the latch register
    let mut c = core_with_isr(38, 0x100); // SFT0I
    c.write_ureg(0x7A, 1 << 28);
    assert_eq!(c.step(), Ok(()));
    assert_eq!(c.pc, 0x100);
}

#[test]
fn irq_and_timer_interrupts_push_the_status_stack_and_mmask_clears_modes() {
    let mut c = core_with_isr(10, 0x100); // IRQ0I
    c.mode1 |= 1 << 21 | 1 << 13; // PEYEN and ALUSAT
    c.mmask = 1 << 21;
    c.flags.az = true;
    c.raise_interrupt(10);
    assert_eq!(c.step(), Ok(()));
    assert_eq!(c.mode1 & (1 << 21), 0, "MMASK bits are cleared in MODE1");
    assert_ne!(c.mode1 & (1 << 13), 0);
    c.flags.az = false;
    assert_eq!(c.step(), Ok(())); // RTI restores
    assert_ne!(c.mode1 & (1 << 21), 0);
    assert!(c.flags.az, "ASTATx comes back from the status stack");
    // a programmable interrupt does not touch the status stack
    let mut c = core_with_isr(11, 0x100);
    c.mode1 |= 1 << 21;
    c.mmask = 1 << 21;
    c.raise_interrupt(11);
    assert_eq!(c.step(), Ok(()));
    assert_ne!(c.mode1 & (1 << 21), 0);
}

#[test]
fn idle_sleeps_until_a_scheduled_interrupt_and_returns_after_the_idle() {
    let mut c = core_with_isr(11, 0x100);
    load(&mut c, 0, &[0x0091, 0x0001]); // IDLE; NOP
    c.schedule_interrupt(1000, 11);
    assert_eq!(c.step(), Ok(())); // IDLE
    assert!(c.is_idle() && c.can_wake());
    assert_eq!(c.step(), Ok(()));
    assert!(c.is_idle());
    assert!(
        c.cycles() >= 999,
        "the idle step fast-forwards to the event"
    );
    assert_eq!(c.step(), Ok(()));
    assert_eq!((c.is_idle(), c.pc), (false, 0x100));
    assert_eq!(c.return_addresses(), &[1], "the instruction after IDLE");
    assert_eq!(c.step(), Ok(())); // RTI
    assert_eq!(c.pc, 1);
}

#[test]
fn idle_with_nothing_to_wake_it_reports_so() {
    let mut c = Core::new(&[]);
    load(&mut c, 0, &[0x0091]);
    assert_eq!(c.step(), Ok(()));
    assert!(c.is_idle() && !c.can_wake());
    assert_eq!(c.run(10), None);
    assert!(c.is_idle());
}

#[test]
fn rsti_is_not_maskable() {
    let mut c = Core::new(&[]);
    c.set_ivt_base(IVT);
    vector_word(&mut c, 1, 1, 0x063e_0000_0500); // reset vector: second word jumps
    c.pc = 0x30;
    assert!(c.raise_interrupt(1));
    assert_eq!(c.step(), Ok(()));
    assert_eq!(c.pc, IVT + VECTOR_PARCELS + 3, "RSTI slot word 0 is a NOP");
    assert_eq!(c.step(), Ok(()));
    assert_eq!(c.pc, 0x500);
}

/// The FX Core1 wake-up path, with the real encodings of vector 15 (P4I) and its routine:
/// the vector saves R0, reads and writes back the status register `0x3108945C` and jumps to
/// the routine, which stores that status in the mailbox `DM(0xC02C1)` the main loop polls.
#[test]
fn firmware_style_mailbox_wake_up_through_p4i() {
    let mut c = Core::new(&[]);
    c.set_ivt_base(IVT);
    let words = [
        0x1100_000c_02c3u64, // dm(0xc02c3) = r0
        0x1000_3108_945c,    // r0 = dm(0x3108945c)
        0x1100_3108_945c,    // dm(0x3108945c) = r0
        0x063e_0012_2fc1,    // jump 0x122fc1 (kept below 0x120000 here)
    ];
    for (k, w) in words.iter().enumerate() {
        let w = if k == 3 { 0x063e_0000_0200 } else { *w };
        vector_word(&mut c, 15, k as u32, w);
    }
    let isr: [u16; 17] = [
        0x1100, 0x000c, 0x02c1, // dm(0xc02c1) = r0
        0x0f80, 0x0001, // r0 = 1
        0x1100, 0x3102, 0x2330, // dm(0x31022330) = r0
        0x1000, 0x000c, 0x02c1, // r0 = dm(0xc02c1)
        0x1000, 0x000c, 0x02c3, // r0 = dm(0xc02c3)
        0x0bbe, 0x0001, 0x0001, // rti
    ];
    load(&mut c, 0x200, &isr);
    // main loop at 0x100: r0 = dm(0xc02c1); r0 = pass r0; if eq jump 0x100; r1 = 5
    load(
        &mut c,
        0x100,
        &[
            0x1000, 0x000c, 0x02c1, 0xc200, 0x0600, 0x0000, 0x0100, 0x0f81, 0x0005,
        ],
    );
    c.pc = 0x100;
    c.r[0] = 0x77;
    c.mode1 |= IRPTEN;
    c.imask |= 1 << 15;
    c.preset_device(0x3108_945C, 1);
    assert_eq!(c.run(40), None);
    assert_eq!(
        (c.r[1], c.pc >= 0x100 && c.pc < 0x108),
        (0, true),
        "still polling"
    );
    c.raise_interrupt(15);
    assert_eq!(c.run(40), None);
    assert_eq!(c.dm_read(0xC_02C1), Ok(1), "the routine stored the status");
    assert_eq!(c.r[1], 5, "the polling loop ended");
    assert_eq!(c.servicing(), None);
}
