//! Secondary register sets selected by MODE1 (`SRRFL/SRRFH`, `SRD1L/SRD1H`, `SRD2L/SRD2H`) and
//! the `SRCU` multiplier-result swap.
//!
//! Model: the visible arrays of [`Core`] always hold the active set; the inactive set lives in
//! [`Banks`]. A MODE1 write that toggles a select bit exchanges the affected registers with the
//! hidden copies, which is equivalent to switching the register file (PRM A-4, A-26) and keeps
//! every other module unaware of the sets. The one-cycle latency before the new set is usable is
//! not modelled.
use super::Core;

const SRCU: u32 = 1 << 2;
/// Select bit and the register range it switches: `(bit, first, end)`.
/// VERIFIED against PRM Table A-1: SRD1H = I/M/L/B4-7, SRD1L = 0-3, SRD2H = 12-15, SRD2L = 8-11.
const DAG_SETS: [(u32, usize, usize); 4] = [
    (1 << 3, 4, 8),
    (1 << 4, 0, 4),
    (1 << 5, 12, 16),
    (1 << 6, 8, 12),
];
/// SRRFH = R8-R15/S8-S15 (bit 7), SRRFL = R0-R7/S0-S7 (bit 10).
const FILE_SETS: [(u32, usize, usize); 2] = [(1 << 7, 8, 16), (1 << 10, 0, 8)];

/// The inactive copies of the switchable registers.
#[derive(Default)]
pub(super) struct Banks {
    r: [u32; 16],
    s: [u32; 16],
    i: [u32; 16],
    m: [u32; 16],
    l: [u32; 16],
    b: [u32; 16],
}

fn swap_range(live: &mut [u32; 16], hidden: &mut [u32; 16], lo: usize, hi: usize) {
    live[lo..hi].swap_with_slice(&mut hidden[lo..hi]);
}

impl Core {
    /// Exchanges the register sets whose select bit differs between `old` and `new` MODE1.
    pub(super) fn apply_mode1_banks(&mut self, old: u32, new: u32) {
        let toggled = old ^ new;
        if toggled & (SRCU | 0x4F8) == 0 {
            return;
        }
        if toggled & SRCU != 0 {
            self.mr.swap(0, 1);
            self.mr_y.swap(0, 1);
        }
        for (bit, lo, hi) in DAG_SETS {
            if toggled & bit != 0 {
                swap_range(&mut self.i, &mut self.banks.i, lo, hi);
                swap_range(&mut self.m, &mut self.banks.m, lo, hi);
                swap_range(&mut self.l, &mut self.banks.l, lo, hi);
                swap_range(&mut self.b, &mut self.banks.b, lo, hi);
            }
        }
        for (bit, lo, hi) in FILE_SETS {
            if toggled & bit != 0 {
                swap_range(&mut self.r, &mut self.banks.r, lo, hi);
                swap_range(&mut self.s, &mut self.banks.s, lo, hi);
            }
        }
    }

    /// Writes MODE1, switching register sets and re-evaluating interrupts (`IRPTEN`, `NESTM`).
    pub(super) fn set_mode1(&mut self, v: u32) {
        let old = self.mode1;
        self.mode1 = v;
        self.apply_mode1_banks(old, v);
        self.poke_irq();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secondary_register_file_keeps_two_independent_sets() {
        let mut c = Core::new(&[]);
        c.r[3] = 11;
        c.r[9] = 22;
        c.write_ureg(0x72, 1 << 10); // SRRFL: R0-R7 secondary
        assert_eq!(
            (c.r[3], c.r[9]),
            (0, 22),
            "low half switched, high half untouched"
        );
        c.r[3] = 33;
        c.write_ureg(0x72, 0);
        assert_eq!(c.r[3], 11, "primary set is back");
        c.write_ureg(0x72, 1 << 10);
        assert_eq!(c.r[3], 33, "secondary set kept its value");
        c.write_ureg(0x72, 1 << 7); // SRRFH instead of SRRFL
        assert_eq!((c.r[3], c.r[9]), (11, 0));
    }

    #[test]
    fn dag_register_sets_switch_in_halves() {
        let mut c = Core::new(&[]);
        (c.i[1], c.m[5], c.l[9], c.b[13]) = (1, 2, 3, 4);
        // SRD1L (bit 4): I/M/L/B0-3; SRD1H (bit 3): 4-7; SRD2L (6): 8-11; SRD2H (5): 12-15
        c.write_ureg(0x72, 1 << 4 | 1 << 3 | 1 << 6 | 1 << 5);
        assert_eq!((c.i[1], c.m[5], c.l[9], c.b[13]), (0, 0, 0, 0));
        c.write_ureg(0x72, 1 << 4);
        assert_eq!((c.i[1], c.m[5], c.l[9], c.b[13]), (0, 2, 3, 4));
    }

    #[test]
    fn firmware_start_up_initialises_both_sets_and_ends_on_the_primary() {
        // the FX start-up: BIT SET MODE1 0x4F8 (all secondary), M1 = 1, BIT CLR MODE1 0x4F8, M1 = 1
        let mut c = Core::new(&[]);
        c.write_ureg(0x72, 0x4F8);
        c.m[1] = 1;
        c.write_ureg(0x72, 0);
        assert_eq!(c.m[1], 0, "the primary M1 was not written yet");
        c.m[1] = 1;
        c.write_ureg(0x72, 0x4F8);
        assert_eq!(c.m[1], 1);
    }

    #[test]
    fn srcu_swaps_the_multiplier_result_registers() {
        let mut c = Core::new(&[]);
        c.mr = [5, 7];
        c.mr_y = [1, 2];
        c.write_ureg(0x72, SRCU);
        assert_eq!((c.mr, c.mr_y), ([7, 5], [2, 1]));
        c.write_ureg(0x72, 0);
        assert_eq!(c.mr, [5, 7]);
    }
}
