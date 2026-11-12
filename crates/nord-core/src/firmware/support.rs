//! Shared synthetic-firmware fixture for the firmware tests.

pub(super) fn build_synthetic_firmware() -> Vec<u8> {
    let mut buf = vec![0u8; 1024];
    // Top-level header (0x20 bytes)
    buf[0..4].copy_from_slice(&0x0000_0020u32.to_be_bytes());
    buf[4..8].copy_from_slice(&0x0000_0140u32.to_be_bytes()); // image0_size = 320
    buf[8..12].copy_from_slice(&0x0000_0160u32.to_be_bytes()); // image1_offset = 0x20 + 0x140 = 0x160
    buf[12..16].copy_from_slice(&0x0000_0100u32.to_be_bytes()); // image1_size = 256

    // Image 0 sub-header at 0x20..0x34: segment_count = 3 at 0x30..0x34
    buf[0x30..0x34].copy_from_slice(&3u32.to_be_bytes());

    // 3 segments of 44 bytes starting at 0x34: SRAM, CODE, ARM0
    for (i, name) in [b"SRAM", b"CODE", b"ARM0"].iter().enumerate() {
        let off = 0x34 + i * 44;
        buf[off..off + 4].copy_from_slice(*name);
        buf[off + 4..off + 8].copy_from_slice(&0x02D4u32.to_be_bytes());
        buf[off + 8..off + 12].copy_from_slice(&0xFC80u32.to_be_bytes());
        buf[off + 12..off + 16].copy_from_slice(&0x8000_0000u32.to_be_bytes());
        buf[off + 16..off + 20].copy_from_slice(&0xFFF3_1619u32.to_be_bytes());
    }

    // Image 1 sub-header at 0x160..0x174: segment_count = 3 at 0x170..0x174
    buf[0x170..0x174].copy_from_slice(&3u32.to_be_bytes());

    // 3 segments of 44 bytes starting at 0x174: SRAM, CODE, ARM0
    for (i, name) in [b"SRAM", b"CODE", b"ARM0"].iter().enumerate() {
        let off = 0x174 + i * 44;
        buf[off..off + 4].copy_from_slice(*name);
        buf[off + 4..off + 8].copy_from_slice(&0x02D4u32.to_be_bytes());
        buf[off + 8..off + 12].copy_from_slice(&0xC158u32.to_be_bytes());
        buf[off + 12..off + 16].copy_from_slice(&0x8000_0000u32.to_be_bytes());
        buf[off + 16..off + 20].copy_from_slice(&0xFFED_0AE8u32.to_be_bytes());
    }

    // Embed SHARC DSP partition markers
    buf[600..608].copy_from_slice(b"DSPFxCor");
    buf[610..620].copy_from_slice(b"DSPFxCore1");
    buf[625..631].copy_from_slice(b"DSPOrg");
    buf[640..646].copy_from_slice(b"DSPSyn");
    buf[650..659].copy_from_slice(b"Fx to Org");
    buf[665..670].copy_from_slice(b"to Fx");
    buf[675..679].copy_from_slice(b"Host");
    buf[685..688].copy_from_slice(b"ORG");
    buf[690..693].copy_from_slice(b"PNO");
    buf[695..698].copy_from_slice(b"SMP");
    buf[700..703].copy_from_slice(b"SY1");
    buf[705..709].copy_from_slice(b"SYL3");
    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::firmware::FirmwareContainer;

    #[test]
    fn synthetic_firmware_parses() {
        assert!(FirmwareContainer::parse(&build_synthetic_firmware()).is_ok());
    }
}
