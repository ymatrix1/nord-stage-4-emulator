//! `.ns4b` backup writer: replaces, adds and removes entries of an existing archive and keeps
//! every untouched entry byte-identical.
//!
//! Spec: `docs/specs/cbin-and-presets.md` section 5.5. The rewrite streams the archive (the
//! Factory Restore backup is 3.14 GB): untouched local-file regions are copied verbatim, a
//! replaced or added entry gets a new `ZIP_STORED` local header with a fresh CRC-32, and the
//! central directory is the original one with the offsets, CRC and sizes of the touched entries
//! patched. `meta.xml` is an ordinary entry: it stays byte-identical unless the caller puts a new
//! one. ZIP64 archives can be copied unchanged but not edited.

use super::backup::Ns4BackupArchive;
use super::container::CbinError;
use super::crc32::crc32;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;

const EOCD_SIG: [u8; 4] = [0x50, 0x4B, 0x05, 0x06];
const CDH_SIG: [u8; 4] = [0x50, 0x4B, 0x01, 0x02];
const LFH_SIG: [u8; 4] = [0x50, 0x4B, 0x03, 0x04];
const CDH_FIXED: usize = 46;
const FLAG_DATA_DESCRIPTOR: u16 = 0x0008;
const FLAG_UTF8_NAME: u16 = 0x0800;
/// DOS date 1980-01-01, used for entries the writer adds.
const DOS_DATE_EPOCH: u16 = 0x0021;
const COPY_CHUNK: usize = 1 << 20;

/// One change to an archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryEdit {
    /// Replace the entry called `name` in place, or append it when the archive has none.
    Put { name: String, data: Vec<u8> },
    /// Drop the entry called `name`.
    Remove { name: String },
}

/// What a rewrite did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RewriteReport {
    pub replaced: usize,
    pub added: usize,
    pub removed: usize,
    pub bytes_written: u64,
}

fn err(msg: impl Into<String>) -> CbinError {
    CbinError::BitstreamError(msg.into())
}

fn io_err(what: &str, e: io::Error) -> CbinError {
    err(format!("{what}: {e}"))
}

fn le16(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn le32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// Output sink that counts the bytes written.
struct Out<'a, W: Write> {
    w: &'a mut W,
    pos: u64,
}

impl<W: Write> Out<'_, W> {
    fn put(&mut self, b: &[u8]) -> Result<(), CbinError> {
        self.w.write_all(b).map_err(|e| io_err("write failed", e))?;
        self.pos += b.len() as u64;
        Ok(())
    }

    fn copy_range<R: Read + Seek>(
        &mut self,
        r: &mut R,
        buf: &mut [u8],
        start: u64,
        len: u64,
    ) -> Result<(), CbinError> {
        r.seek(SeekFrom::Start(start))
            .map_err(|e| io_err("seek failed", e))?;
        let mut left = len;
        while left > 0 {
            let n = left.min(buf.len() as u64) as usize;
            r.read_exact(&mut buf[..n])
                .map_err(|e| io_err("read failed", e))?;
            self.put(&buf[..n])?;
            left -= n as u64;
        }
        Ok(())
    }
}

fn u32_offset(v: u64, what: &str) -> Result<u32, CbinError> {
    u32::try_from(v)
        .ok()
        .filter(|&x| x != u32::MAX)
        .ok_or_else(|| {
            err(format!(
                "{what} exceeds 4 GB: ZIP64 archives cannot be edited"
            ))
        })
}

/// Local file header + data of a `ZIP_STORED` entry.
fn local_entry(
    version: [u8; 2],
    flags: u16,
    time: u16,
    date: u16,
    name: &str,
    data: &[u8],
) -> Result<Vec<u8>, CbinError> {
    let size = u32_offset(data.len() as u64, "entry size")?;
    let mut v = Vec::with_capacity(30 + name.len() + data.len());
    v.extend_from_slice(&LFH_SIG);
    v.extend_from_slice(&version);
    v.extend_from_slice(&flags.to_le_bytes());
    v.extend_from_slice(&0u16.to_le_bytes()); // method: stored
    v.extend_from_slice(&time.to_le_bytes());
    v.extend_from_slice(&date.to_le_bytes());
    v.extend_from_slice(&crc32(data).to_le_bytes());
    v.extend_from_slice(&size.to_le_bytes());
    v.extend_from_slice(&size.to_le_bytes());
    v.extend_from_slice(&(name.len() as u16).to_le_bytes());
    v.extend_from_slice(&0u16.to_le_bytes()); // extra length
    v.extend_from_slice(name.as_bytes());
    v.extend_from_slice(data);
    Ok(v)
}

/// Central directory record of a freshly added entry.
fn new_central_record(name: &str, data: &[u8], local_offset: u32, flags: u16) -> Vec<u8> {
    let mut v = Vec::with_capacity(CDH_FIXED + name.len());
    v.extend_from_slice(&CDH_SIG);
    v.extend_from_slice(&20u16.to_le_bytes()); // version made by
    v.extend_from_slice(&20u16.to_le_bytes()); // version needed
    v.extend_from_slice(&flags.to_le_bytes());
    v.extend_from_slice(&0u16.to_le_bytes());
    v.extend_from_slice(&0u16.to_le_bytes()); // time
    v.extend_from_slice(&DOS_DATE_EPOCH.to_le_bytes());
    v.extend_from_slice(&crc32(data).to_le_bytes());
    v.extend_from_slice(&(data.len() as u32).to_le_bytes());
    v.extend_from_slice(&(data.len() as u32).to_le_bytes());
    v.extend_from_slice(&(name.len() as u16).to_le_bytes());
    v.extend_from_slice(&[0u8; 12]); // extra, comment, disk, internal, external attributes
    v.extend_from_slice(&local_offset.to_le_bytes());
    v.extend_from_slice(name.as_bytes());
    v
}

/// The edits sorted into what the rewrite needs.
struct Plan<'a> {
    put: HashMap<&'a str, &'a [u8]>,
    remove: HashSet<&'a str>,
    /// Puts whose name the archive does not have, in edit order.
    added: Vec<(&'a str, &'a [u8])>,
}

fn plan<'a>(archive: &Ns4BackupArchive, edits: &'a [EntryEdit]) -> Result<Plan<'a>, CbinError> {
    let existing: HashSet<&str> = archive.entries().iter().map(|e| e.name.as_str()).collect();
    let mut p = Plan {
        put: HashMap::new(),
        remove: HashSet::new(),
        added: Vec::new(),
    };
    for e in edits {
        match e {
            EntryEdit::Put { name, data } => {
                if p.remove.contains(name.as_str()) || p.put.insert(name, data).is_some() {
                    return Err(err(format!("conflicting edits for '{name}'")));
                }
                if !existing.contains(name.as_str()) {
                    p.added.push((name, data));
                }
            }
            EntryEdit::Remove { name } => {
                if !existing.contains(name.as_str()) {
                    return Err(err(format!("cannot remove '{name}': not in the archive")));
                }
                if p.put.contains_key(name.as_str()) || !p.remove.insert(name) {
                    return Err(err(format!("conflicting edits for '{name}'")));
                }
            }
        }
    }
    Ok(p)
}

/// Splits the raw central directory into one slice per record.
fn split_central(cd: &[u8], count: usize) -> Result<Vec<&[u8]>, CbinError> {
    let mut recs = Vec::with_capacity(count);
    let mut p = 0usize;
    while p < cd.len() {
        if p + CDH_FIXED > cd.len() || cd[p..p + 4] != CDH_SIG {
            return Err(err("malformed central directory"));
        }
        let end = p
            + CDH_FIXED
            + usize::from(le16(cd, p + 28))
            + usize::from(le16(cd, p + 30))
            + usize::from(le16(cd, p + 32));
        if end > cd.len() {
            return Err(err("truncated central directory record"));
        }
        recs.push(&cd[p..end]);
        p = end;
    }
    if recs.len() != count {
        return Err(err("central directory record count mismatch"));
    }
    Ok(recs)
}

/// Rewrites `archive` (read through `reader`) into `writer` with `edits` applied. With no edits
/// the output is a verbatim copy, ZIP64 archives included.
pub fn rewrite_backup<R: Read + Seek, W: Write>(
    archive: &Ns4BackupArchive,
    reader: &mut R,
    writer: &mut W,
    edits: &[EntryEdit],
) -> Result<RewriteReport, CbinError> {
    let mut out = Out { w: writer, pos: 0 };
    let mut buf = vec![0u8; COPY_CHUNK];
    let mut report = RewriteReport::default();
    if edits.is_empty() {
        let len = reader
            .seek(SeekFrom::End(0))
            .map_err(|e| io_err("seek failed", e))?;
        out.copy_range(reader, &mut buf, 0, len)?;
        report.bytes_written = out.pos;
        return Ok(report);
    }
    if archive.is_zip64() {
        return Err(err("ZIP64 archives cannot be edited"));
    }
    let plan = plan(archive, edits)?;
    let entries = archive.entries();
    let (cd_off, cd_size) = (
        archive.central_directory_offset(),
        archive.central_directory_size(),
    );

    let mut cd = vec![0u8; cd_size as usize];
    reader
        .seek(SeekFrom::Start(cd_off))
        .and_then(|_| reader.read_exact(&mut cd))
        .map_err(|e| io_err("read central directory failed", e))?;
    let mut tail = Vec::new();
    reader
        .read_to_end(&mut tail)
        .map_err(|e| io_err("read archive tail failed", e))?;
    if tail.len() < 22 || tail[..4] != EOCD_SIG {
        return Err(err(
            "unsupported layout: no end record right after the central directory",
        ));
    }
    let recs = split_central(&cd, entries.len())?;

    // Local-file regions in file order; entry k spans [offset_k, offset_k+1).
    let mut order: Vec<usize> = (0..entries.len()).collect();
    order.sort_by_key(|&i| entries[i].local_header_offset);
    let first = order
        .first()
        .map_or(cd_off, |&i| entries[i].local_header_offset);
    out.copy_range(reader, &mut buf, 0, first)?;
    let mut new_off: Vec<Option<u32>> = vec![None; entries.len()];
    let mut patched: HashMap<usize, (u32, u32)> = HashMap::new(); // idx -> (crc, size)
    for (k, &i) in order.iter().enumerate() {
        let e = &entries[i];
        let end = order
            .get(k + 1)
            .map_or(cd_off, |&j| entries[j].local_header_offset);
        if plan.remove.contains(e.name.as_str()) {
            report.removed += 1;
            continue;
        }
        new_off[i] = Some(u32_offset(out.pos, "local header offset")?);
        match plan.put.get(e.name.as_str()) {
            None => out.copy_range(
                reader,
                &mut buf,
                e.local_header_offset,
                end - e.local_header_offset,
            )?,
            Some(data) => {
                let r = recs[i];
                if le32(r, 20) == u32::MAX || le32(r, 24) == u32::MAX {
                    return Err(err(format!(
                        "'{}' uses ZIP64 sizes and cannot be replaced",
                        e.name
                    )));
                }
                let flags = le16(r, 8) & !FLAG_DATA_DESCRIPTOR;
                let version = [r[6], r[7]];
                let bytes = local_entry(version, flags, le16(r, 12), le16(r, 14), &e.name, data)?;
                out.put(&bytes)?;
                patched.insert(i, (crc32(data), data.len() as u32));
                report.replaced += 1;
            }
        }
    }
    // Entries appended after the existing local-file region.
    let mut added_off = Vec::new();
    for (name, data) in &plan.added {
        added_off.push(u32_offset(out.pos, "local header offset")?);
        let flags = if name.is_ascii() { 0 } else { FLAG_UTF8_NAME };
        out.put(&local_entry([20, 0], flags, 0, DOS_DATE_EPOCH, name, data)?)?;
        report.added += 1;
    }

    let cd_start = u32_offset(out.pos, "central directory offset")?;
    let mut count = 0usize;
    for (i, r) in recs.iter().enumerate() {
        let Some(off) = new_off[i] else { continue };
        let mut rec = r.to_vec();
        rec[42..46].copy_from_slice(&off.to_le_bytes());
        if let Some(&(crc, size)) = patched.get(&i) {
            let flags = le16(r, 8) & !FLAG_DATA_DESCRIPTOR;
            rec[8..10].copy_from_slice(&flags.to_le_bytes());
            rec[10..12].copy_from_slice(&0u16.to_le_bytes());
            rec[16..20].copy_from_slice(&crc.to_le_bytes());
            rec[20..24].copy_from_slice(&size.to_le_bytes());
            rec[24..28].copy_from_slice(&size.to_le_bytes());
        }
        out.put(&rec)?;
        count += 1;
    }
    for ((name, data), off) in plan.added.iter().zip(&added_off) {
        let flags = if name.is_ascii() { 0 } else { FLAG_UTF8_NAME };
        out.put(&new_central_record(name, data, *off, flags))?;
        count += 1;
    }
    let cd_len = u32_offset(out.pos - u64::from(cd_start), "central directory size")?;
    let count16 = u16::try_from(count)
        .ok()
        .filter(|&c| c != u16::MAX)
        .ok_or_else(|| err("too many entries for a non-ZIP64 archive"))?;
    let mut eocd = tail;
    eocd[8..10].copy_from_slice(&count16.to_le_bytes());
    eocd[10..12].copy_from_slice(&count16.to_le_bytes());
    eocd[12..16].copy_from_slice(&cd_len.to_le_bytes());
    eocd[16..20].copy_from_slice(&cd_start.to_le_bytes());
    out.put(&eocd)?;
    report.bytes_written = out.pos;
    Ok(report)
}

/// [`rewrite_backup`] from `src` to `dst` on disk. `dst` must differ from `src`; a failed write
/// removes the partial output.
pub fn rewrite_backup_file(
    src: &Path,
    dst: &Path,
    edits: &[EntryEdit],
) -> Result<RewriteReport, CbinError> {
    if let (Ok(a), Ok(b)) = (src.canonicalize(), dst.canonicalize()) {
        if a == b {
            return Err(err("output must differ from the input backup"));
        }
    }
    let archive = Ns4BackupArchive::open(src)?;
    let mut reader = BufReader::new(File::open(src).map_err(|e| io_err("open input failed", e))?);
    let mut writer =
        BufWriter::new(File::create(dst).map_err(|e| io_err("create output failed", e))?);
    let result = rewrite_backup(&archive, &mut reader, &mut writer, edits).and_then(|r| {
        writer
            .flush()
            .map(|_| r)
            .map_err(|e| io_err("flush failed", e))
    });
    if result.is_err() {
        drop(writer);
        let _ = std::fs::remove_file(dst);
    }
    result
}

#[cfg(test)]
#[path = "backup_writer_tests.rs"]
pub(crate) mod more_tests;

#[cfg(test)]
mod tests {
    use super::more_tests::build_zip;
    use super::*;
    use std::io::Cursor;

    #[test]
    fn put_replaces_in_place_and_leaves_the_rest_alone() {
        let zip = build_zip(&[("a.bin", b"one"), ("b.bin", b"two"), ("c.bin", b"333")]);
        let archive = Ns4BackupArchive::from_bytes(&zip).unwrap();
        let edit = [EntryEdit::Put {
            name: "b.bin".into(),
            data: b"TWO".to_vec(),
        }];
        let mut out = Vec::new();
        let rep = rewrite_backup(&archive, &mut Cursor::new(&zip), &mut out, &edit).unwrap();
        assert_eq!((rep.replaced, rep.added, rep.removed), (1, 0, 0));
        let back = Ns4BackupArchive::from_bytes(&out).unwrap();
        let mut cur = Cursor::new(&out);
        assert_eq!(back.read_entry(&mut cur, "b.bin").unwrap(), b"TWO");
        assert_eq!(back.read_entry(&mut cur, "c.bin").unwrap(), b"333");
        assert_eq!(out.len(), zip.len());
    }
}
