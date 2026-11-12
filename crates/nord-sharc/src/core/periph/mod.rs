//! Peripheral models behind the data-memory window of [`Core`] (`docs/research/sharc-chip-identity.md`).
//!
//! Two families, both optional and off by default so the `--dev` scripted device store of the
//! harnesses keeps working unchanged (a scripted `preset` always wins over a model):
//!
//! - [`Sc57x`]: the ADSP-2157x register blocks the FX, PNO and ORG images use (`0x3xxxxxxx`) and
//!   L2 SRAM (`0x2000_0000`): SEC0, SPI2, descriptor-list DMA, the TRU routing between DMA
//!   channels, LP1 and L2 backing store.
//! - [`SpiBoot`]: the SPI-slave DMA of the ADSP-2148x boot kernel of the SY1SY2 and SMP images.
//!
//! Peripheral register behaviour that was derived from CCES headers and image code is tagged
//! VERIFIED in the submodules; everything about timing and side effects is ASSUMED until measured
//! on the CCES simulator (probe list in `sharc-chip-identity.md` section 6).
pub mod dma;
pub mod l2;
pub mod sec;
pub mod spi2;
pub mod spi_boot;

mod glue;
pub mod sc57x;

pub use sc57x::Sc57x;
pub use spi_boot::SpiBoot;

/// Word access to the memory behind a DMA channel.
pub trait Bus {
    fn read32(&mut self, addr: u32) -> u32;
    fn write32(&mut self, addr: u32, value: u32);
}

/// Core interrupt vector the SEC drives (ASSUMED: `P4I`, the vector whose ORG ISR reads `CSID1`).
/// Both peripheral families of a [`Core`](super::Core).
#[derive(Default)]
pub struct Periph {
    pub sc57x: Option<Sc57x>,
    pub boot: Option<SpiBoot>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peripherals_are_off_by_default() {
        let p = Periph::default();
        assert!(p.sc57x.is_none() && p.boot.is_none());
    }
}
