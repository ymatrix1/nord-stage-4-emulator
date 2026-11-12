//! ADSP-214xx VISA instruction classifier and text decoder.
//!
//! Port of `tools/sharc_visa.py` (same output, so both can be diffed on the real DSP images).
//! Layouts come from the SHARC Processor Programming Reference rev 2.4, chapters 9 to 12; the
//! parcel order (16-bit little-endian parcels, most significant parcel first) and the type
//! lengths are VERIFIED on the DSP images (`docs/research/dsp-image-hunt.md` 5.5 and 5.6). Field
//! positions not confirmed by the data are tagged ASSUMED in the Python tool and here.
//!
//! Layout: `types` holds [`Kind`] and [`Insn`], `classify` the parcel level (type and length,
//! tiling, field extraction), `regs` the register and condition names, `compute` the text of
//! compute fields and `decode` the per-type operand text and the listing line. The public API is
//! re-exported here, so callers use `crate::visa::{classify, decode_text, ...}`.

mod classify;
mod compute;
mod decode;
mod regs;
mod types;

pub use classify::{bits, classify, parcels_le, tile, ureg_load, word48};
pub use compute::{compute_text, mult_text, multi_text, short_compute_text};
pub use decode::{decode_text, listing_line};
pub use regs::ureg_name;
pub use types::{Insn, Kind};
