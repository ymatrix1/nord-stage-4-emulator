//! Firmware container tests: header, checksum, partitions, payload extraction.

use super::support::*;
use super::*;

#[test]
fn test_parse_firmware_container_and_sharc_tags() {
    let raw = build_synthetic_firmware();
    let fw = FirmwareContainer::parse(&raw).expect("synthetic firmware should parse");
    assert_eq!(fw.header_size, 0x20);
    assert_eq!(fw.image0_size, 0x140);
    assert_eq!(fw.image1_offset, 0x160);
    assert_eq!(fw.image0_segments.len(), 3);
    assert_eq!(fw.image1_segments.len(), 3);
    assert_eq!(fw.segments.len(), 3);
    assert_eq!(fw.segments[0].name, "SRAM");
    assert_eq!(fw.segments[1].name, "CODE");
    assert_eq!(fw.segments[2].name, "ARM0");

    let sram = fw.segment("SRAM").expect("SRAM segment should exist");
    assert_eq!(sram.target_address, 0x8000_0000);
    assert_eq!(sram.bank, 0);
    assert!(fw.segment("NON_EXISTENT").is_none());

    let img1_code = fw
        .segment_in_bank(1, "CODE")
        .expect("Image 1 CODE segment should exist");
    assert_eq!(img1_code.bank, 1);
    assert_eq!(img1_code.data_size, 0xC158);

    let tags: Vec<&str> = fw.dsp_partitions.iter().map(|p| p.tag.as_str()).collect();
    assert!(tags.contains(&"DSPO"));
    assert!(tags.contains(&"DSPS"));
    assert!(tags.contains(&"DSPF"));
    assert!(tags.contains(&"DSPOrg"));
    assert!(tags.contains(&"DSPSyn"));
    assert!(tags.contains(&"DSPFxCore1"));
    assert!(tags.contains(&"Fx to Org"));
    assert!(tags.contains(&"Host"));
    assert!(tags.contains(&"ORG"));
    assert!(tags.contains(&"PNO"));
    assert!(tags.contains(&"SMP"));
}

#[test]
fn test_calculate_checksum_and_verify() {
    // Deterministic synthetic buffer
    let data = vec![0x10u8, 0x20u8, 0x30u8, 0x40u8];
    let csum = calculate_checksum(&data);
    assert_eq!(csum, 0xFFFF_FFFF - (0x10 + 0x20 + 0x30 + 0x40));

    let seg = FirmwareSegment {
        name: "TEST".to_string(),
        bank: 0,
        data_offset: 0,
        data_size: 4,
        target_address: 0,
        checksum: csum,
    };
    assert!(seg.verify_checksum(&data));
    assert!(!seg.verify_checksum(&[0x10, 0x20, 0x30, 0x41]));
}

#[test]
fn test_dsp_partitions_all_engines() {
    let raw = build_synthetic_firmware();
    let fw = FirmwareContainer::parse(&raw).expect("synthetic firmware should parse");

    let organ_part = fw
        .dsp_partitions
        .iter()
        .find(|p| p.tag == "DSPOrg")
        .expect("DSPOrg partition should exist");
    assert_eq!(organ_part.engine_type, DspEngineType::Organ);

    let synth_part = fw
        .dsp_partitions
        .iter()
        .find(|p| p.tag == "DSPSyn")
        .expect("DSPSyn partition should exist");
    assert_eq!(synth_part.engine_type, DspEngineType::Synth);

    let fx_part = fw
        .dsp_partitions
        .iter()
        .find(|p| p.tag == "DSPFxCore1")
        .expect("DSPFxCore1 partition should exist");
    assert_eq!(fx_part.engine_type, DspEngineType::Effects);

    let route_part = fw
        .dsp_partitions
        .iter()
        .find(|p| p.tag == "Fx to Org")
        .expect("Fx to Org routing should exist");
    assert_eq!(route_part.engine_type, DspEngineType::Routing);

    let core_part = fw
        .dsp_partitions
        .iter()
        .find(|p| p.tag == "PNO")
        .expect("PNO core should exist");
    assert_eq!(core_part.engine_type, DspEngineType::CoreIdentifier);
}

#[test]
fn test_rejects_invalid_firmware_header() {
    assert!(FirmwareContainer::parse(&[0u8; 8]).is_err());
    let mut bad = build_synthetic_firmware();
    bad[3] = 0x99;
    assert!(FirmwareContainer::parse(&bad).is_err());
}

#[test]
fn test_extract_payload_bounds() {
    let seg = FirmwareSegment {
        name: "SRAM".to_string(),
        bank: 0,
        data_offset: 500,
        data_size: 100,
        target_address: 0x8000_0000,
        checksum: 0,
    };
    let small_buf = vec![0u8; 200];
    assert!(seg.extract_payload(&small_buf).is_err());
}

#[test]
fn test_extract_payload_code_segment_decompression() {
    let raw_code = b"ARM926EJ-S startup code payload with repeating patterns: 00 80 e3 00 f0 21 e1 00 80 e3 00 f0 21 e1";
    let compressed_code = compress_clavia_lz(raw_code);

    let offset = 64usize;
    let mut firmware_raw = vec![0u8; offset + compressed_code.len() + 32];
    firmware_raw[offset..offset + compressed_code.len()].copy_from_slice(&compressed_code);

    let code_seg = FirmwareSegment {
        name: "CODE".to_string(),
        bank: 0,
        data_offset: offset as u32,
        data_size: raw_code.len() as u32,
        target_address: 0xC000_0000,
        checksum: calculate_checksum(raw_code),
    };

    let decompressed = code_seg
        .extract_payload(&firmware_raw)
        .expect("extract_payload on CODE segment should decompress successfully");
    assert_eq!(decompressed, raw_code);
    assert!(code_seg.verify_checksum(&decompressed));
}

#[test]
fn test_extract_segment_respects_bank_base_offsets() {
    let bank0_code = b"Bank0 CODE segment LZO1X payload repeating LZO1X payload!";
    let comp0 = compress_clavia_lz(bank0_code);
    let bank1_code = b"Bank1 CODE segment LZO1X payload repeating LZO1X payload!";
    let comp1 = compress_clavia_lz(bank1_code);

    let mut buf = vec![0u8; 2048];
    // Header: 0x20 header_size, bank0 starts at 0x20, bank1 starts at 0x400
    let img0_rel_code = 0x180usize;
    let img1_offset = 0x400usize;
    let img1_rel_code = 0x180usize;
    buf[0..4].copy_from_slice(&0x20u32.to_be_bytes());
    buf[4..8].copy_from_slice(&((img1_offset - 0x20) as u32).to_be_bytes());
    buf[8..12].copy_from_slice(&(img1_offset as u32).to_be_bytes());
    buf[12..16].copy_from_slice(&0x400u32.to_be_bytes());

    // Bank 0 segment table
    buf[0x30..0x34].copy_from_slice(&1u32.to_be_bytes());
    buf[0x34..0x38].copy_from_slice(b"CODE");
    buf[0x38..0x3C].copy_from_slice(&(img0_rel_code as u32).to_be_bytes());
    buf[0x3C..0x40].copy_from_slice(&(bank0_code.len() as u32).to_be_bytes());
    buf[0x40..0x44].copy_from_slice(&0xC000_0000u32.to_be_bytes());
    buf[0x44..0x48].copy_from_slice(&calculate_checksum(bank0_code).to_be_bytes());
    buf[0x20 + img0_rel_code..0x20 + img0_rel_code + comp0.len()].copy_from_slice(&comp0);

    // Bank 1 segment table
    buf[img1_offset + 0x10..img1_offset + 0x14].copy_from_slice(&1u32.to_be_bytes());
    let s1 = img1_offset + 0x14;
    buf[s1..s1 + 4].copy_from_slice(b"CODE");
    buf[s1 + 4..s1 + 8].copy_from_slice(&(img1_rel_code as u32).to_be_bytes());
    buf[s1 + 8..s1 + 12].copy_from_slice(&(bank1_code.len() as u32).to_be_bytes());
    buf[s1 + 12..s1 + 16].copy_from_slice(&0xC000_0000u32.to_be_bytes());
    buf[s1 + 16..s1 + 20].copy_from_slice(&calculate_checksum(bank1_code).to_be_bytes());
    buf[img1_offset + img1_rel_code..img1_offset + img1_rel_code + comp1.len()]
        .copy_from_slice(&comp1);

    let fw = FirmwareContainer::parse(&buf).expect("parse should succeed");
    let out0 = fw
        .extract_segment(0, "CODE", &buf)
        .expect("bank 0 CODE extraction");
    let out1 = fw
        .extract_segment(1, "CODE", &buf)
        .expect("bank 1 CODE extraction");
    assert_eq!(out0, bank0_code);
    assert_eq!(out1, bank1_code);
    assert!(fw
        .segment_in_bank(0, "CODE")
        .unwrap()
        .verify_checksum(&out0));
    assert!(fw
        .segment_in_bank(1, "CODE")
        .unwrap()
        .verify_checksum(&out1));
}
