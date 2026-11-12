//! Test helpers shared by the unit tests of the `core` modules.
use super::Core;

/// Short-word address where [`core_with`] places its program (inside the VISA code window).
pub(super) const CODE: u32 = 0x12_0000;

/// Writes parcels (little-endian, most significant parcel first) at a short-word address.
pub(super) fn load(core: &mut Core, short: u32, parcels: &[u16]) {
    for (k, p) in parcels.iter().enumerate() {
        let o = (short as usize + k) * 2;
        core.sram[o..o + 2].copy_from_slice(&p.to_le_bytes());
    }
}

/// A core with `program` loaded and the pc at [`CODE`].
pub(super) fn core_with(program: &[u16]) -> Core {
    let mut c = Core::new(&[]);
    load(&mut c, CODE, program);
    c.pc = CODE;
    c
}

/// A core with `parcels` at short-word 0 and the pc there.
pub(super) fn core_at_zero(parcels: &[u16]) -> Core {
    let mut c = Core::new(&[]);
    load(&mut c, 0, parcels);
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers_place_parcels_little_endian() {
        let c = core_with(&[0x1234, 0x5678]);
        assert_eq!(
            (c.pc, c.parcel(CODE), c.parcel(CODE + 1)),
            (CODE, 0x1234, 0x5678)
        );
        assert_eq!(core_at_zero(&[0xBEEF]).parcel(0), 0xBEEF);
    }
}
