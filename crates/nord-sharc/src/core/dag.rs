//! Data address generators: index modify with circular buffers and the DAG1/DAG2 bank offset.
use super::Core;

impl Core {
    /// Register offset of a DAG selector bit: DAG1 uses `I0..I7`/`M0..M7`, DAG2 uses `I8..I15`.
    #[inline]
    pub(super) fn dag_bank(bit: u64) -> usize {
        if bit == 1 {
            8
        } else {
            0
        }
    }

    /// Next DAG index after modifying `i` by `m` with circular-buffer length `l` and base `b`.
    #[inline]
    pub(super) fn dag_next(i: u32, m: u32, l: u32, b: u32) -> u32 {
        let next = i.wrapping_add(m);
        if l == 0 {
            return next;
        }
        if (m as i32) >= 0 {
            if next >= b.wrapping_add(l) {
                next.wrapping_sub(l)
            } else {
                next
            }
        } else if next < b {
            next.wrapping_add(l)
        } else {
            next
        }
    }

    /// Index register `ii` modified by modify register `mi` (honours its `L` and `B`).
    #[inline]
    pub(super) fn modified(&self, ii: usize, mi: usize) -> u32 {
        Self::dag_next(self.i[ii], self.m[mi], self.l[ii], self.b[ii])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dag_addressing_modes_and_circular_buffers() {
        assert_eq!(Core::dag_next(10, 1, 0, 0), 11);
        assert_eq!(Core::dag_next(10, 0xFFFF_FFFF, 0, 0), 9);
        // circular buffer base 100 length 4: 103 + 1 wraps to 100, 100 - 1 wraps to 103
        assert_eq!(Core::dag_next(103, 1, 4, 100), 100);
        assert_eq!(Core::dag_next(100, 0xFFFF_FFFF, 4, 100), 103);
    }

    #[test]
    fn bank_offset_and_register_modify() {
        assert_eq!((Core::dag_bank(0), Core::dag_bank(1)), (0, 8));
        let mut c = Core::new(&[]);
        (c.i[9], c.m[10], c.l[9], c.b[9]) = (103, 1, 4, 100);
        assert_eq!(c.modified(9, 10), 100);
    }
}
