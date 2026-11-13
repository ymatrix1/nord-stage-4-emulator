//! Shared test fixtures: synthetic `.ns4b` (PKZIP) archives written to the temp dir.

use std::path::PathBuf;

/// `meta.xml` payload decoding to "v166 build 2934".
pub const META: &[u8] = b"<backup file_version=\"1\" backup_format_version=\"1\" product_id=\"46\" product_version=\"166\" product_build=\"2934\" manager_version=\"786\" manager_build=\"1234\"/>";

/// Builds a minimal STORED-method PKZIP archive from `(name, bytes)` pairs (CRC left 0).
pub fn build_synthetic_zip_stored(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();

    for (name, data) in files {
        let local_offset = out.len() as u32;
        let name_bytes = name.as_bytes();
        let crc = 0u32;
        let size = data.len() as u32;

        out.extend_from_slice(&[0x50, 0x4B, 0x03, 0x04]);
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name_bytes);
        out.extend_from_slice(data);

        central.extend_from_slice(&[0x50, 0x4B, 0x01, 0x02]);
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u32.to_le_bytes());
        central.extend_from_slice(&local_offset.to_le_bytes());
        central.extend_from_slice(name_bytes);
    }

    let cd_offset = out.len() as u32;
    let cd_size = central.len() as u32;
    let count = files.len() as u16;
    out.extend_from_slice(&central);

    out.extend_from_slice(&[0x50, 0x4B, 0x05, 0x06]);
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&cd_size.to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

/// Writes a synthetic `.ns4b` to the temp dir (unique per process and `tag`) and returns its path.
pub fn write_temp_ns4b(tag: &str, files: &[(&str, &[u8])]) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("ns4_gui_{tag}_{}.ns4b", std::process::id()));
    std::fs::write(&path, build_synthetic_zip_stored(files)).expect("write temp ns4b");
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use nord_core::cbin::Ns4BackupArchive;

    #[test]
    fn test_write_temp_ns4b_round_trips_through_archive_reader() {
        let path = write_temp_ns4b("support", &[("meta.xml", META), ("a/b.ns4p", b"xyz")]);
        let archive = Ns4BackupArchive::open(&path).expect("open synthetic archive");
        assert!(archive.entries().iter().any(|e| e.name == "a/b.ns4p"));
        let _ = std::fs::remove_file(&path);
    }
}
