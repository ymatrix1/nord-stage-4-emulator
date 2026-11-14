//! Tests of the `.ns4b` writer on synthetic archives shaped like the Factory Restore backup:
//! `ZIP_STORED`, no data descriptors, a one-byte extra field in every central record.

use super::*;
use std::io::Cursor;

/// Archive with real CRCs, DOS time/date set and a 1-byte central-directory extra field.
pub(crate) fn build_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut cd = Vec::new();
    for (name, data) in files {
        let off = out.len() as u32;
        let crc = crc32(data);
        let n = data.len() as u32;
        out.extend_from_slice(&LFH_SIG);
        for v in [20u16, 0, 0, 0x8479, 0x5CD8] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for v in [crc, n, n] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);

        cd.extend_from_slice(&CDH_SIG);
        for v in [0x031Eu16, 20, 0, 0, 0x8479, 0x5CD8] {
            cd.extend_from_slice(&v.to_le_bytes());
        }
        for v in [crc, n, n] {
            cd.extend_from_slice(&v.to_le_bytes());
        }
        cd.extend_from_slice(&(name.len() as u16).to_le_bytes());
        cd.extend_from_slice(&1u16.to_le_bytes()); // extra length
        cd.extend_from_slice(&[0u8; 10]); // comment, disk, internal, external
        cd.extend_from_slice(&off.to_le_bytes());
        cd.extend_from_slice(name.as_bytes());
        cd.push(0xAB);
    }
    let cd_off = out.len() as u32;
    let count = files.len() as u16;
    out.extend_from_slice(&cd);
    out.extend_from_slice(&EOCD_SIG);
    out.extend_from_slice(&[0u8; 4]);
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&(cd.len() as u32).to_le_bytes());
    out.extend_from_slice(&cd_off.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

fn put(name: &str, data: &[u8]) -> EntryEdit {
    EntryEdit::Put {
        name: name.into(),
        data: data.to_vec(),
    }
}

fn rewrite(zip: &[u8], edits: &[EntryEdit]) -> Result<(Vec<u8>, RewriteReport), CbinError> {
    let archive = Ns4BackupArchive::from_bytes(zip)?;
    let mut out = Vec::new();
    let rep = rewrite_backup(&archive, &mut Cursor::new(zip), &mut out, edits)?;
    Ok((out, rep))
}

/// Every entry of `zip` as (name, data, crc recorded in the central directory).
fn dump(zip: &[u8]) -> Vec<(String, Vec<u8>, u32)> {
    let a = Ns4BackupArchive::from_bytes(zip).unwrap();
    let mut cur = Cursor::new(zip);
    (0..a.entries().len())
        .map(|i| {
            let e = &a.entries()[i];
            (
                e.name.clone(),
                a.read_entry_by_index(&mut cur, i).unwrap(),
                e.crc32,
            )
        })
        .collect()
}

fn sample() -> Vec<u8> {
    build_zip(&[
        ("meta.xml", b"<backup product_id=\"46\"/>"),
        ("Program/Bank A/One.ns4p", &[1u8; 868]),
        ("Program/Bank A/Two.ns4p", &[2u8; 868]),
        ("Live/Bank 1/Live 0.ns4l", &[3u8; 868]),
    ])
}

#[test]
fn no_edits_is_a_byte_identical_copy() {
    let zip = sample();
    let (out, rep) = rewrite(&zip, &[]).unwrap();
    assert_eq!(out, zip);
    assert_eq!(rep.bytes_written, zip.len() as u64);
}

#[test]
fn same_size_replace_changes_only_that_entrys_crc_and_data() {
    let zip = sample();
    let new = [9u8; 868];
    let (out, rep) = rewrite(&zip, &[put("Program/Bank A/Two.ns4p", &new)]).unwrap();
    assert_eq!(out.len(), zip.len());
    assert_eq!((rep.replaced, rep.added, rep.removed), (1, 0, 0));
    let differing: Vec<usize> = (0..zip.len()).filter(|&i| zip[i] != out[i]).collect();
    // Local header of "Two": 30 + 23 name bytes after the first two entries.
    let a = Ns4BackupArchive::from_bytes(&zip).unwrap();
    let lfh = a.find("Two.ns4p").unwrap().local_header_offset as usize;
    let data = lfh + 30 + "Program/Bank A/Two.ns4p".len();
    let cdh_crc = (a.central_directory_offset() as usize..zip.len())
        .step_by(1)
        .find(|&i| {
            zip[i..].starts_with(b"PK\x01\x02") && zip[i + 46..].starts_with(b"Program/Bank A/Two")
        })
        .unwrap()
        + 16;
    for i in &differing {
        let in_crc = |base: usize| (base..base + 4).contains(i);
        assert!(
            (data..data + 868).contains(i) || in_crc(lfh + 14) || in_crc(cdh_crc),
            "unexpected difference at {i}"
        );
    }
    let before = dump(&zip);
    let after = dump(&out);
    for (b, a) in before.iter().zip(&after) {
        if a.0 == "Program/Bank A/Two.ns4p" {
            assert_eq!(a.1, new);
            assert_eq!(a.2, crc32(&new));
        } else {
            assert_eq!(a, b, "{} must be untouched", a.0);
        }
    }
}

#[test]
fn different_size_replace_shifts_offsets_and_keeps_the_rest() {
    let zip = sample();
    let (out, _) = rewrite(
        &zip,
        &[put("meta.xml", b"<backup product_id=\"46\" extra=\"1\"/>")],
    )
    .unwrap();
    assert!(out.len() > zip.len());
    let (before, after) = (dump(&zip), dump(&out));
    assert_eq!(after[0].1, b"<backup product_id=\"46\" extra=\"1\"/>");
    assert_eq!(&before[1..], &after[1..]);
    // Region bytes of untouched entries are verbatim, central records included.
    assert!(out.windows(868).any(|w| w == [3u8; 868]));
}

#[test]
fn add_and_remove_and_rename() {
    let zip = sample();
    let edits = [
        put("Program/Bank B/New.ns4p", &[7u8; 868]),
        EntryEdit::Remove {
            name: "Program/Bank A/One.ns4p".into(),
        },
        EntryEdit::Remove {
            name: "Live/Bank 1/Live 0.ns4l".into(),
        },
        put("Live/Bank 1/Live 0 \u{e9}.ns4l", &[4u8; 868]),
    ];
    let (out, rep) = rewrite(&zip, &edits).unwrap();
    assert_eq!((rep.replaced, rep.added, rep.removed), (0, 2, 2));
    let names: Vec<String> = dump(&out).into_iter().map(|e| e.0).collect();
    assert_eq!(
        names,
        [
            "meta.xml",
            "Program/Bank A/Two.ns4p",
            "Program/Bank B/New.ns4p",
            "Live/Bank 1/Live 0 \u{e9}.ns4l"
        ]
    );
    for (n, d, crc) in dump(&out) {
        assert_eq!(crc, crc32(&d), "{n}");
    }
    // The result can be edited again (idempotent structure).
    let (again, _) = rewrite(&out, &[]).unwrap();
    assert_eq!(again, out);
}

#[test]
fn bad_edits_are_rejected() {
    let zip = sample();
    let gone = EntryEdit::Remove {
        name: "nope".into(),
    };
    assert!(rewrite(&zip, &[gone]).is_err());
    let both = [
        put("meta.xml", b"x"),
        EntryEdit::Remove {
            name: "meta.xml".into(),
        },
    ];
    assert!(rewrite(&zip, &both).is_err());
    assert!(rewrite(&zip, &[put("meta.xml", b"x"), put("meta.xml", b"y")]).is_err());
}

#[test]
fn rewrite_file_refuses_the_same_path_and_writes_a_valid_archive() {
    let dir = std::env::temp_dir().join(format!("ns4-writer-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (src, dst) = (dir.join("in.ns4b"), dir.join("out.ns4b"));
    std::fs::write(&src, sample()).unwrap();
    assert!(rewrite_backup_file(&src, &src, &[put("meta.xml", b"x")]).is_err());
    rewrite_backup_file(&src, &dst, &[put("meta.xml", b"<backup/>")]).unwrap();
    let out = std::fs::read(&dst).unwrap();
    assert_eq!(dump(&out)[0].1, b"<backup/>");
    let bad = rewrite_backup_file(
        &src,
        &dir.join("bad.ns4b"),
        &[EntryEdit::Remove { name: "zz".into() }],
    );
    assert!(bad.is_err() && !dir.join("bad.ns4b").exists());
}
