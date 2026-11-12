//! Clavia LZ codec tests: round trips, token structure and bounds protection.

use super::*;

#[test]
fn test_clavia_lz_roundtrip_synthetic() {
    let original = b"Nord Stage 4 OS Firmware LZ Token Compressor/Decompressor Verification Test! Repeating: 123456789 123456789 123456789 AAAAAAAAAAAAAAAAAAAA";
    let compressed = compress_clavia_lz(original);
    assert!(compressed.len() < original.len());

    let decompressed = decompress_clavia_lz(&compressed, original.len() * 2)
        .expect("decompression should succeed");
    assert_eq!(decompressed, original);
}

#[test]
fn test_clavia_lz_token_bit_structure() {
    // Real ARM926EJ-S reset prefix compressed with LZO1X (verified against OS v1.66 Image 0/1):
    // 00 1B = extended initial literal run of 3 + 15 + 27 = 45 bytes
    // DC 04 = M2 match: len = (0xDC >> 5) + 1 = 7, dist = 1 + ((0xDC >> 2) & 7) + (0x04 << 3) = 40
    // 01 70 D0 9F E5 = literal run of 3 + 1 = 4 bytes
    // ED 02 17 = M2 match len 8, dist 20, + 1 trailing literal (0x17)
    // CD 02 60 = M2 match len 7, dist 20, + 1 trailing literal (0x60)
    // 29 4D 00 1B = M3 match len 11, dist 20, + 1 trailing literal (0x1B)
    // 11 00 00 = LZO1X EOF
    let arm_stub: [u8; 45] = [
        0x00, 0x00, 0x0f, 0xe1, 0xc0, 0x00, 0x80, 0xe3, 0x00, 0xf0, 0x21, 0xe1, 0x00, 0x00, 0xa0,
        0xe3, 0x17, 0x0f, 0x07, 0xee, 0x9a, 0x0f, 0x07, 0xee, 0x10, 0x0f, 0x11, 0xee, 0x01, 0x0a,
        0x80, 0xe3, 0x10, 0x0f, 0x01, 0xee, 0x00, 0x00, 0x0f, 0xe1, 0x1f, 0x00, 0xc0, 0xe3, 0x13,
    ];
    let mut stream = vec![0x00u8, 0x1Bu8];
    stream.extend_from_slice(&arm_stub);
    stream.extend_from_slice(&[
        0xdc, 0x04, 0x01, 0x70, 0xd0, 0x9f, 0xe5, 0xed, 0x02, 0x17, 0xcd, 0x02, 0x60, 0x29, 0x4d,
        0x00, 0x11, 0x00, 0x00,
    ]);

    let out = decompress_clavia_lz(&stream, 256).expect("ARM stub LZO1X should decode");
    assert_eq!(&out[0..45], &arm_stub);
    // SVC mode: orr r0,r0,#0x13; msr cpsr_c,r0; ldr sp,[pc,#0x70]
    assert_eq!(
        &out[45..56],
        &[0x00, 0x80, 0xe3, 0x00, 0xf0, 0x21, 0xe1, 0x70, 0xd0, 0x9f, 0xe5]
    );
    // ABT mode setup: mrs r0,cpsr; bic r0,r0,#0x1f; orr r0,r0,#0x17; msr cpsr_c,r0; ldr sp,[pc,#0x60]
    assert_eq!(
        &out[56..76],
        &[
            0x00, 0x00, 0x0f, 0xe1, 0x1f, 0x00, 0xc0, 0xe3, 0x17, 0x00, 0x80, 0xe3, 0x00, 0xf0,
            0x21, 0xe1, 0x60, 0xd0, 0x9f, 0xe5
        ]
    );
}

#[test]
fn test_clavia_lz_bounds_protection() {
    // Initial stream containing M2 match exceeding empty output buffer should fail
    // Use initial 1-byte literal prefix (17 + 1 = 18) followed by M2 match (0x40, 0x05 -> dist 41 > 1)
    let bad_dist = vec![18u8, 0xAA, 0x40, 0x05, 0x11, 0x00, 0x00];
    assert!(decompress_clavia_lz(&bad_dist, 100).is_err());

    // Max output overflow should fail
    let tiny_max = vec![22u8, 1, 2, 3, 4, 5, 0x11, 0x00, 0x00];
    assert!(decompress_clavia_lz(&tiny_max, 3).is_err());
}

#[test]
fn test_clavia_lz_empty_and_single_byte() {
    let empty: &[u8] = b"";
    let c_empty = compress_clavia_lz(empty);
    let d_empty = decompress_clavia_lz(&c_empty, 10).expect("empty should decompress");
    assert_eq!(d_empty, empty);

    for sample in [b"Z".as_slice(), b"XY".as_slice(), b"XYZ".as_slice()] {
        let c_single = compress_clavia_lz(sample);
        let d_single =
            decompress_clavia_lz(&c_single, 10).expect("short literal should decompress");
        assert_eq!(d_single, sample);
    }
}

#[test]
fn test_clavia_lz_all_match_lengths_3_through_8() {
    for target_len in 3..=8 {
        let mut data = vec![b'B'; 30];
        let pattern: Vec<u8> = (0..target_len as u8).map(|i| b'K' + i).collect();
        data.extend_from_slice(&pattern);
        data.extend_from_slice(&pattern);
        data.extend_from_slice(b"TAIL");

        let compressed = compress_clavia_lz(&data);
        let decompressed = decompress_clavia_lz(&compressed, data.len() * 2)
            .expect("decompression of match length test must succeed");
        assert_eq!(
            decompressed, data,
            "failed roundtrip for match length {target_len}"
        );
    }
}

#[test]
fn test_clavia_lz_extended_literal_run() {
    // LZO1X extended initial literal run: 0x00, 0x1B => 3 + 15 + 27 = 45 literal bytes + EOF
    let mut stream = vec![0x00u8, 0x1Bu8];
    let literal_bytes = vec![0xA5u8; 45];
    stream.extend_from_slice(&literal_bytes);
    stream.extend_from_slice(&[0x11, 0x00, 0x00]);

    let out = decompress_clavia_lz(&stream, 100)
        .expect("extended literal run decompression should succeed");
    assert_eq!(out, literal_bytes);
}

#[test]
fn test_clavia_lz_truncated_token_error() {
    // M2 match byte with missing high offset byte should fail
    let truncated = vec![18u8, 0xAA, 0x40];
    assert!(decompress_clavia_lz(&truncated, 100).is_err());
}
