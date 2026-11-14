//! Zero-dependency PKZIP (`ZIP_STORED`) `.ns4b` Backup Archive Reader.
//!
//! Spec: `docs/specs/cbin-and-presets.md` §5.4, `docs/research/nsmp4-and-npno-cnsp-codec.md` §1

use super::container::CbinError;
use super::manifest::BackupManifest;
use std::fs::File;
use std::io::{BufReader, Cursor, Read, Seek, SeekFrom};
use std::path::Path;

const EOCD_SIG: [u8; 4] = [0x50, 0x4B, 0x05, 0x06];
const ZIP64_LOCATOR_SIG: [u8; 4] = [0x50, 0x4B, 0x06, 0x07];
const ZIP64_EOCD_SIG: [u8; 4] = [0x50, 0x4B, 0x06, 0x06];
const CDH_SIG: [u8; 4] = [0x50, 0x4B, 0x01, 0x02];
const LFH_SIG: [u8; 4] = [0x50, 0x4B, 0x03, 0x04];

/// PKZIP `ZIP_STORED` compression method (uncompressed).
pub const ZIP_METHOD_STORED: u16 = 0;

/// Metadata for a single file entry inside an `.ns4b` PKZIP archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ns4BackupEntry {
    /// Forward-slash normalized archive path (e.g., `meta.xml`, `Piano/White Grand.npno`).
    pub name: String,
    /// PKZIP compression method (`0` = `ZIP_STORED`).
    pub compression_method: u16,
    /// CRC-32 checksum recorded in the central directory.
    pub crc32: u32,
    /// Compressed byte count.
    pub compressed_size: u64,
    /// Uncompressed byte count.
    pub uncompressed_size: u64,
    /// File byte offset of the Local File Header (`0x04034B50`).
    pub local_header_offset: u64,
}

/// Central-directory index for a `.ns4b` Nord Stage 4 backup archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ns4BackupArchive {
    entries: Vec<Ns4BackupEntry>,
    cd_offset: u64,
    cd_size: u64,
    zip64: bool,
}

impl Ns4BackupArchive {
    /// Opens an `.ns4b` archive from disk and parses its PKZIP central directory.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, CbinError> {
        let file = File::open(path.as_ref())
            .map_err(|e| CbinError::BitstreamError(format!("failed to open .ns4b file: {e}")))?;
        let mut reader = BufReader::new(file);
        Self::parse(&mut reader)
    }

    /// Parses an in-memory `.ns4b` PKZIP byte slice.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CbinError> {
        let mut cursor = Cursor::new(bytes);
        Self::parse(&mut cursor)
    }

    /// Parses PKZIP central directory entries from any seekable reader.
    pub fn parse<R: Read + Seek>(reader: &mut R) -> Result<Self, CbinError> {
        let file_len = reader
            .seek(SeekFrom::End(0))
            .map_err(|e| CbinError::BitstreamError(format!("seek end failed: {e}")))?;
        if file_len < 22 {
            return Err(CbinError::BitstreamError(
                ".ns4b archive too small for PKZIP EOCD".to_string(),
            ));
        }

        let tail_len = file_len.min(65_557) as usize;
        let tail_start = file_len - tail_len as u64;
        reader
            .seek(SeekFrom::Start(tail_start))
            .map_err(|e| CbinError::BitstreamError(format!("seek tail failed: {e}")))?;
        let mut tail = vec![0u8; tail_len];
        reader
            .read_exact(&mut tail)
            .map_err(|e| CbinError::BitstreamError(format!("read tail failed: {e}")))?;

        let mut eocd_rel = None;
        if tail.len() >= 22 {
            for i in (0..=tail.len() - 22).rev() {
                if tail[i..i + 4] == EOCD_SIG {
                    let comment_len = u16::from_le_bytes([tail[i + 20], tail[i + 21]]) as usize;
                    if i + 22 + comment_len == tail.len() {
                        eocd_rel = Some(i);
                        break;
                    }
                }
            }
        }
        let eocd_rel = eocd_rel.ok_or_else(|| {
            CbinError::BitstreamError("missing PKZIP End of Central Directory".to_string())
        })?;
        let eocd = &tail[eocd_rel..eocd_rel + 22];

        let mut total_entries = u64::from(u16::from_le_bytes([eocd[10], eocd[11]]));
        let mut cd_size = u64::from(u32::from_le_bytes([eocd[12], eocd[13], eocd[14], eocd[15]]));
        let mut cd_offset = u64::from(u32::from_le_bytes([eocd[16], eocd[17], eocd[18], eocd[19]]));
        let mut zip64 = false;

        // Optional ZIP64 EOCD locator immediately before standard EOCD
        if cd_offset == u64::from(u32::MAX)
            || cd_size == u64::from(u32::MAX)
            || total_entries == 0xFFFF
        {
            let eocd_abs = tail_start + eocd_rel as u64;
            if eocd_abs >= 20 {
                let mut loc = [0u8; 20];
                reader.seek(SeekFrom::Start(eocd_abs - 20)).map_err(|e| {
                    CbinError::BitstreamError(format!("seek zip64 locator failed: {e}"))
                })?;
                reader.read_exact(&mut loc).map_err(|e| {
                    CbinError::BitstreamError(format!("read zip64 locator failed: {e}"))
                })?;
                if loc[0..4] == ZIP64_LOCATOR_SIG {
                    let z64_off = u64::from_le_bytes(loc[8..16].try_into().unwrap());
                    let mut z64 = [0u8; 56];
                    reader.seek(SeekFrom::Start(z64_off)).map_err(|e| {
                        CbinError::BitstreamError(format!("seek zip64 eocd failed: {e}"))
                    })?;
                    reader.read_exact(&mut z64).map_err(|e| {
                        CbinError::BitstreamError(format!("read zip64 eocd failed: {e}"))
                    })?;
                    if z64[0..4] != ZIP64_EOCD_SIG {
                        return Err(CbinError::BitstreamError(
                            "invalid ZIP64 EOCD signature".to_string(),
                        ));
                    }
                    total_entries = u64::from_le_bytes(z64[32..40].try_into().unwrap());
                    cd_size = u64::from_le_bytes(z64[40..48].try_into().unwrap());
                    cd_offset = u64::from_le_bytes(z64[48..56].try_into().unwrap());
                    zip64 = true;
                }
            }
        }

        if cd_offset.saturating_add(cd_size) > file_len {
            return Err(CbinError::BitstreamError(
                "PKZIP central directory out of file bounds".to_string(),
            ));
        }

        let cd_len = usize::try_from(cd_size)
            .map_err(|_| CbinError::BitstreamError("central directory too large".to_string()))?;
        let mut cd = vec![0u8; cd_len];
        reader
            .seek(SeekFrom::Start(cd_offset))
            .map_err(|e| CbinError::BitstreamError(format!("seek CD failed: {e}")))?;
        reader
            .read_exact(&mut cd)
            .map_err(|e| CbinError::BitstreamError(format!("read CD failed: {e}")))?;

        let mut entries = Vec::with_capacity(total_entries.min(65_536) as usize);
        let mut p = 0usize;
        while p + 46 <= cd.len() {
            if cd[p..p + 4] != CDH_SIG {
                return Err(CbinError::BitstreamError(format!(
                    "unexpected signature in PKZIP central directory at +{p}"
                )));
            }
            let compression_method = u16::from_le_bytes([cd[p + 10], cd[p + 11]]);
            let crc32 = u32::from_le_bytes([cd[p + 16], cd[p + 17], cd[p + 18], cd[p + 19]]);
            let mut compressed_size = u64::from(u32::from_le_bytes([
                cd[p + 20],
                cd[p + 21],
                cd[p + 22],
                cd[p + 23],
            ]));
            let mut uncompressed_size = u64::from(u32::from_le_bytes([
                cd[p + 24],
                cd[p + 25],
                cd[p + 26],
                cd[p + 27],
            ]));
            let name_len = u16::from_le_bytes([cd[p + 28], cd[p + 29]]) as usize;
            let extra_len = u16::from_le_bytes([cd[p + 30], cd[p + 31]]) as usize;
            let comment_len = u16::from_le_bytes([cd[p + 32], cd[p + 33]]) as usize;
            let mut local_header_offset = u64::from(u32::from_le_bytes([
                cd[p + 42],
                cd[p + 43],
                cd[p + 44],
                cd[p + 45],
            ]));

            let rec_end = p + 46 + name_len + extra_len + comment_len;
            if rec_end > cd.len() {
                return Err(CbinError::BitstreamError(
                    "truncated PKZIP central directory record".to_string(),
                ));
            }
            let name = String::from_utf8_lossy(&cd[p + 46..p + 46 + name_len]).into_owned();
            let extra = &cd[p + 46 + name_len..p + 46 + name_len + extra_len];
            apply_zip64_extra(
                extra,
                &mut uncompressed_size,
                &mut compressed_size,
                &mut local_header_offset,
            );

            entries.push(Ns4BackupEntry {
                name,
                compression_method,
                crc32,
                compressed_size,
                uncompressed_size,
                local_header_offset,
            });
            p = rec_end;
        }

        Ok(Self {
            entries,
            cd_offset,
            cd_size,
            zip64,
        })
    }

    /// File offset of the central directory (the end of the local-file region).
    pub fn central_directory_offset(&self) -> u64 {
        self.cd_offset
    }

    /// Byte length of the central directory.
    pub fn central_directory_size(&self) -> u64 {
        self.cd_size
    }

    /// The archive carries a ZIP64 end-of-central-directory record.
    pub fn is_zip64(&self) -> bool {
        self.zip64
    }

    /// All parsed entries in archive central-directory order.
    pub fn entries(&self) -> &[Ns4BackupEntry] {
        &self.entries
    }

    /// Finds an entry by exact path or case-insensitive filename suffix match.
    pub fn find(&self, query: &str) -> Option<&Ns4BackupEntry> {
        if let Some(exact) = self.entries.iter().find(|e| e.name == query) {
            return Some(exact);
        }
        let q_lower = query.to_ascii_lowercase();
        self.entries
            .iter()
            .find(|e| e.name.to_ascii_lowercase().ends_with(&q_lower))
    }

    /// Lists all entries ending with `ext` (case-insensitive, e.g. `".npno"`).
    pub fn entries_with_ext(&self, ext: &str) -> Vec<&Ns4BackupEntry> {
        let ext_lower = ext.to_ascii_lowercase();
        self.entries
            .iter()
            .filter(|e| e.name.to_ascii_lowercase().ends_with(&ext_lower))
            .collect()
    }

    /// Reads an uncompressed (`ZIP_STORED`) entry by exact index from a seekable reader.
    pub fn read_entry_by_index<R: Read + Seek>(
        &self,
        reader: &mut R,
        idx: usize,
    ) -> Result<Vec<u8>, CbinError> {
        let entry = self
            .entries
            .get(idx)
            .ok_or_else(|| CbinError::BitstreamError(format!("entry index {idx} out of range")))?;
        if entry.compression_method != ZIP_METHOD_STORED {
            return Err(CbinError::BitstreamError(format!(
                "unsupported compression method {} for '{}' (expected ZIP_STORED = 0)",
                entry.compression_method, entry.name
            )));
        }

        reader
            .seek(SeekFrom::Start(entry.local_header_offset))
            .map_err(|e| CbinError::BitstreamError(format!("seek LFH failed: {e}")))?;
        let mut lfh = [0u8; 30];
        reader
            .read_exact(&mut lfh)
            .map_err(|e| CbinError::BitstreamError(format!("read LFH failed: {e}")))?;
        if lfh[0..4] != LFH_SIG {
            return Err(CbinError::BitstreamError(format!(
                "invalid Local File Header signature for '{}'",
                entry.name
            )));
        }

        let name_len = u16::from_le_bytes([lfh[26], lfh[27]]) as u64;
        let extra_len = u16::from_le_bytes([lfh[28], lfh[29]]) as u64;
        let data_offset = entry.local_header_offset + 30 + name_len + extra_len;
        let size = usize::try_from(entry.uncompressed_size)
            .map_err(|_| CbinError::BitstreamError("entry exceeds host usize".to_string()))?;

        reader
            .seek(SeekFrom::Start(data_offset))
            .map_err(|e| CbinError::BitstreamError(format!("seek entry payload failed: {e}")))?;
        let mut buf = vec![0u8; size];
        reader
            .read_exact(&mut buf)
            .map_err(|e| CbinError::BitstreamError(format!("read entry payload failed: {e}")))?;
        Ok(buf)
    }

    /// Finds and reads an uncompressed entry matching `query` from a seekable reader.
    pub fn read_entry<R: Read + Seek>(
        &self,
        reader: &mut R,
        query: &str,
    ) -> Result<Vec<u8>, CbinError> {
        let idx = self
            .entries
            .iter()
            .position(|e| e.name == query)
            .or_else(|| {
                let q_lower = query.to_ascii_lowercase();
                self.entries
                    .iter()
                    .position(|e| e.name.to_ascii_lowercase().ends_with(&q_lower))
            })
            .ok_or_else(|| {
                CbinError::BitstreamError(format!("backup entry '{query}' not found"))
            })?;
        self.read_entry_by_index(reader, idx)
    }

    /// Opens `path` and reads a single entry matching `query`.
    pub fn read_entry_from_file<P: AsRef<Path>>(
        &self,
        path: P,
        query: &str,
    ) -> Result<Vec<u8>, CbinError> {
        let file = File::open(path.as_ref())
            .map_err(|e| CbinError::BitstreamError(format!("failed to open .ns4b file: {e}")))?;
        let mut reader = BufReader::new(file);
        self.read_entry(&mut reader, query)
    }

    /// Finds an archive entry ending with `ext` (e.g. `".npno"` or `".nsmp4"`) whose filename stem
    /// matches `target_hash` via OS v1.66 `nord_name_hash` (including uppercase fallback).
    pub fn find_by_name_hash(&self, ext: &str, target_hash: u32) -> Option<&Ns4BackupEntry> {
        if target_hash == 0 {
            return None;
        }
        let ext_lower = ext.to_ascii_lowercase();
        self.entries.iter().find(|e| {
            e.name.to_ascii_lowercase().ends_with(&ext_lower)
                && Path::new(&e.name)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .is_some_and(|stem| super::presets::nord_name_hash_matches(stem, target_hash))
        })
    }

    /// Resolves a `.ns4p` Piano `0x0B` `name_hash` to its matching `.npno` archive entry.
    pub fn find_piano_by_hash(&self, name_hash: u32) -> Option<&Ns4BackupEntry> {
        self.find_by_name_hash(".npno", name_hash)
    }

    /// Resolves a `.ns4p` Synth `0x32` `sample_id` to its matching `.nsmp4` archive entry.
    pub fn find_sample_by_id(&self, sample_id: u32) -> Option<&Ns4BackupEntry> {
        self.find_by_name_hash(".nsmp4", sample_id)
    }

    /// Resolves referenced `.npno` and `.nsmp4` entry paths inside the archive for a decoded program.
    pub fn resolve_program_sounds(
        &self,
        prog: &super::presets::ProgramDecoded,
    ) -> ProgramSoundRefs {
        let piano_a = self
            .find_piano_by_hash(prog.piano.layer_a.name_hash)
            .map(|e| e.name.clone());
        let piano_b = self
            .find_piano_by_hash(prog.piano.layer_b.name_hash)
            .map(|e| e.name.clone());
        let resolve_synth = |layer: &super::synth_preset::SynthLayerDecoded| {
            if layer.osc_mode == crate::synth::SynthOscMode::Sample {
                self.find_sample_by_id(layer.sample_id)
                    .map(|e| e.name.clone())
            } else {
                None
            }
        };
        ProgramSoundRefs {
            piano_a,
            piano_b,
            synth_a: resolve_synth(&prog.synth.layer_a),
            synth_b: resolve_synth(&prog.synth.layer_b),
            synth_c: resolve_synth(&prog.synth.layer_c),
        }
    }

    /// Reads and parses the root `meta.xml` backup manifest from the archive.
    pub fn read_manifest<R: Read + Seek>(
        &self,
        reader: &mut R,
    ) -> Result<BackupManifest, CbinError> {
        let xml_bytes = self.read_entry(reader, "meta.xml")?;
        let xml_str = std::str::from_utf8(&xml_bytes)
            .map_err(|e| CbinError::BitstreamError(format!("meta.xml is not valid UTF-8: {e}")))?;
        BackupManifest::parse_xml(xml_str)
    }
}

/// Resolved archive entry paths for a decoded `.ns4p` program's Piano (`.npno`) and Synth (`.nsmp4`) banks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProgramSoundRefs {
    pub piano_a: Option<String>,
    pub piano_b: Option<String>,
    pub synth_a: Option<String>,
    pub synth_b: Option<String>,
    pub synth_c: Option<String>,
}

fn apply_zip64_extra(
    extra: &[u8],
    uncompressed_size: &mut u64,
    compressed_size: &mut u64,
    local_header_offset: &mut u64,
) {
    let mut i = 0usize;
    while i + 4 <= extra.len() {
        let id = u16::from_le_bytes([extra[i], extra[i + 1]]);
        let sz = u16::from_le_bytes([extra[i + 2], extra[i + 3]]) as usize;
        let end = i + 4 + sz;
        if end > extra.len() {
            break;
        }
        if id == 0x0001 {
            let mut k = i + 4;
            if *uncompressed_size == u64::from(u32::MAX) && k + 8 <= end {
                *uncompressed_size = u64::from_le_bytes(extra[k..k + 8].try_into().unwrap());
                k += 8;
            }
            if *compressed_size == u64::from(u32::MAX) && k + 8 <= end {
                *compressed_size = u64::from_le_bytes(extra[k..k + 8].try_into().unwrap());
                k += 8;
            }
            if *local_header_offset == u64::from(u32::MAX) && k + 8 <= end {
                *local_header_offset = u64::from_le_bytes(extra[k..k + 8].try_into().unwrap());
            }
            break;
        }
        i = end;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_synthetic_zip(entries: &[(&str, u16, &[u8])]) -> Vec<u8> {
        let mut archive = Vec::new();
        let mut cd = Vec::new();

        for &(name, method, data) in entries {
            let lfh_offset = archive.len() as u32;
            let name_bytes = name.as_bytes();
            let size = data.len() as u32;

            // Local file header (30 bytes + name)
            archive.extend_from_slice(&LFH_SIG);
            archive.extend_from_slice(&20u16.to_le_bytes()); // version needed
            archive.extend_from_slice(&0u16.to_le_bytes()); // flags
            archive.extend_from_slice(&method.to_le_bytes());
            archive.extend_from_slice(&0u16.to_le_bytes()); // time
            archive.extend_from_slice(&0u16.to_le_bytes()); // date
            archive.extend_from_slice(&0x1234_5678u32.to_le_bytes()); // crc32
            archive.extend_from_slice(&size.to_le_bytes());
            archive.extend_from_slice(&size.to_le_bytes());
            archive.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
            archive.extend_from_slice(&0u16.to_le_bytes()); // extra len
            archive.extend_from_slice(name_bytes);
            archive.extend_from_slice(data);

            // Central directory header (46 bytes + name)
            cd.extend_from_slice(&CDH_SIG);
            cd.extend_from_slice(&20u16.to_le_bytes());
            cd.extend_from_slice(&20u16.to_le_bytes());
            cd.extend_from_slice(&0u16.to_le_bytes());
            cd.extend_from_slice(&method.to_le_bytes());
            cd.extend_from_slice(&0u16.to_le_bytes());
            cd.extend_from_slice(&0u16.to_le_bytes());
            cd.extend_from_slice(&0x1234_5678u32.to_le_bytes());
            cd.extend_from_slice(&size.to_le_bytes());
            cd.extend_from_slice(&size.to_le_bytes());
            cd.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
            cd.extend_from_slice(&0u16.to_le_bytes()); // extra len
            cd.extend_from_slice(&0u16.to_le_bytes()); // comment len
            cd.extend_from_slice(&0u16.to_le_bytes()); // disk start
            cd.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
            cd.extend_from_slice(&0u32.to_le_bytes()); // external attrs
            cd.extend_from_slice(&lfh_offset.to_le_bytes());
            cd.extend_from_slice(name_bytes);
        }

        let cd_offset = archive.len() as u32;
        let cd_size = cd.len() as u32;
        archive.extend_from_slice(&cd);

        // End of central directory (22 bytes)
        let count = entries.len() as u16;
        archive.extend_from_slice(&EOCD_SIG);
        archive.extend_from_slice(&0u16.to_le_bytes());
        archive.extend_from_slice(&0u16.to_le_bytes());
        archive.extend_from_slice(&count.to_le_bytes());
        archive.extend_from_slice(&count.to_le_bytes());
        archive.extend_from_slice(&cd_size.to_le_bytes());
        archive.extend_from_slice(&cd_offset.to_le_bytes());
        archive.extend_from_slice(&0u16.to_le_bytes()); // comment len

        archive
    }

    #[test]
    fn test_ns4b_archive_parses_manifest_and_reads_stored_entries() {
        let meta = br#"<backup file_version="2" backup_format_version="1" product_id="46" product_version="166" product_build="12" manager_version="900" manager_build="1" />"#;
        let fake_npno = b"CBIN\x01\x00\x00\x00npno_payload";
        let fake_nsmp4 = b"CBIN\x01\x00\x00\x00nsmp_payload";
        let zip_bytes = build_synthetic_zip(&[
            ("meta.xml", ZIP_METHOD_STORED, meta),
            ("Piano/White Grand.npno", ZIP_METHOD_STORED, fake_npno),
            ("Samp Lib/Solo Violin.nsmp4", ZIP_METHOD_STORED, fake_nsmp4),
        ]);

        let archive = Ns4BackupArchive::from_bytes(&zip_bytes).expect("parse synthetic ns4b");
        assert_eq!(archive.entries().len(), 3);
        assert_eq!(archive.entries_with_ext(".npno").len(), 1);
        assert_eq!(archive.entries_with_ext(".NSMP4").len(), 1);

        let mut cursor = Cursor::new(&zip_bytes);
        let manifest = archive.read_manifest(&mut cursor).expect("parse manifest");
        assert!(manifest.is_nord_stage_4());
        assert_eq!(manifest.product_id, 46);
        assert_eq!(manifest.product_version, 166);

        let got_npno = archive
            .read_entry(&mut cursor, "White Grand.npno")
            .expect("suffix lookup");
        assert_eq!(got_npno.as_slice(), fake_npno);
    }

    #[test]
    fn test_ns4b_archive_rejects_invalid_and_compressed_entries() {
        assert!(Ns4BackupArchive::from_bytes(b"not a zip").is_err());

        let deflated = build_synthetic_zip(&[("Program/Test.ns4p", 8, b"compressed")]);
        let archive = Ns4BackupArchive::from_bytes(&deflated).expect("cd parses");
        let mut cursor = Cursor::new(&deflated);
        assert!(archive
            .read_entry(&mut cursor, "Program/Test.ns4p")
            .is_err());
        assert!(archive.read_entry(&mut cursor, "missing.ns4p").is_err());
    }

    #[test]
    fn test_ns4b_archive_resolves_program_hashes() {
        let piano_stem = "Astoria Grand Lrg 6.1";
        let sample_stem = "Ac Guitar Finger_ST Mart 4.1";
        let zip_bytes = build_synthetic_zip(&[
            (
                "Piano/Grand/Astoria Grand Lrg 6.1.npno",
                ZIP_METHOD_STORED,
                b"npno",
            ),
            (
                "Samp Lib/Guitar/Ac Guitar Finger_ST Mart 4.1.nsmp4",
                ZIP_METHOD_STORED,
                b"nsmp",
            ),
        ]);
        let archive = Ns4BackupArchive::from_bytes(&zip_bytes).expect("parse synthetic ns4b");
        let mut prog = super::super::presets::ProgramDecoded::default();
        prog.piano.layer_a.name_hash = super::super::presets::nord_name_hash(piano_stem);
        prog.synth.layer_b.osc_mode = crate::synth::SynthOscMode::Sample;
        prog.synth.layer_b.sample_id = super::super::presets::nord_name_hash(sample_stem);

        let refs = archive.resolve_program_sounds(&prog);
        assert_eq!(
            refs.piano_a.as_deref(),
            Some("Piano/Grand/Astoria Grand Lrg 6.1.npno")
        );
        assert_eq!(refs.piano_b, None);
        assert_eq!(refs.synth_a, None);
        assert_eq!(
            refs.synth_b.as_deref(),
            Some("Samp Lib/Guitar/Ac Guitar Finger_ST Mart 4.1.nsmp4")
        );
        assert_eq!(refs.synth_c, None);
    }
}
