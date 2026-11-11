//! Clavia Binary (`CBIN`) v1 container and `.ns4b` backup manifest parser/serializer.
//!
//! Spec: `docs/specs/cbin-and-presets.md`

use std::fmt;

/// Fixed 4-byte Clavia Binary magic header (`"CBIN"`).
pub const CBIN_MAGIC: [u8; 4] = *b"CBIN";

/// Fixed 48-byte (`0x30`) header size for all `.ns4*` preset files.
pub const CBIN_HEADER_SIZE: usize = 48;

/// Nord Stage 4 hardware product identifier in `meta.xml` (`46` = `0x2E`).
pub const NS4_PRODUCT_ID: u32 = 46;

/// Recognized Clavia Binary file tags in a Nord Stage 4 system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CbinKind {
    /// `.ns4p` — Full 7-layer Program (`868` bytes, schema `0x0139`).
    Program,
    /// `.ns4l` — Live buffer slot (`868` bytes, schema `0x0139`).
    Live,
    /// `.ns4y` — Single-layer Synth preset (`541` bytes, schema `0x00D0`).
    SynthPreset,
    /// `.ns4n` — Single-layer Piano preset (`195` bytes, schema `0x00CB`).
    PianoPreset,
    /// `.ns4o` — Single-layer Organ preset (`183` bytes, schema `0x00CD`).
    OrganPreset,
    /// `.ns4t` — System settings table (`124` bytes, schema `0x006C`).
    Settings,
    /// `.npno` — Nord Piano Library container.
    PianoSample,
    /// `.nsmp` / `.nsmp4` — Nord Sample Library 4 container.
    SampleLibrary,
}

impl CbinKind {
    /// Parses the 4-byte ASCII tag at offset `0x08..0x0C`.
    pub fn from_tag(tag: &[u8; 4]) -> Option<Self> {
        match tag {
            b"ns4p" => Some(Self::Program),
            b"ns4l" => Some(Self::Live),
            b"ns4y" => Some(Self::SynthPreset),
            b"ns4n" => Some(Self::PianoPreset),
            b"ns4o" => Some(Self::OrganPreset),
            b"ns4t" => Some(Self::Settings),
            b"npno" => Some(Self::PianoSample),
            b"nsmp" => Some(Self::SampleLibrary),
            _ => None,
        }
    }

    /// Returns the 4-byte ASCII tag for this file type.
    pub fn tag(&self) -> [u8; 4] {
        match self {
            Self::Program => *b"ns4p",
            Self::Live => *b"ns4l",
            Self::SynthPreset => *b"ns4y",
            Self::PianoPreset => *b"ns4n",
            Self::OrganPreset => *b"ns4o",
            Self::Settings => *b"ns4t",
            Self::PianoSample => *b"npno",
            Self::SampleLibrary => *b"nsmp",
        }
    }

    /// Expected fixed total file size in bytes for preset files (`None` for variable sample banks).
    pub fn expected_file_size(&self) -> Option<usize> {
        match self {
            Self::Program | Self::Live => Some(868),
            Self::SynthPreset => Some(541),
            Self::PianoPreset => Some(195),
            Self::OrganPreset => Some(183),
            Self::Settings => Some(124),
            Self::PianoSample | Self::SampleLibrary => None,
        }
    }

    /// Expected schema identifier (`+0x14` LE and `+0x2E` BE) for Stage 4 OS v1.66 preset files.
    pub fn expected_schema_id(&self) -> Option<u16> {
        match self {
            Self::Program | Self::Live => Some(0x0139),
            Self::SynthPreset => Some(0x00D0),
            Self::PianoPreset => Some(0x00CB),
            Self::OrganPreset => Some(0x00CD),
            Self::Settings => Some(0x006C),
            Self::PianoSample | Self::SampleLibrary => None,
        }
    }
}

/// Errors returned when parsing or validating a `CBIN` file or `.ns4b` manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CbinError {
    TooShort {
        actual: usize,
        min: usize,
    },
    InvalidMagic([u8; 4]),
    UnsupportedVersion(u32),
    UnknownTag([u8; 4]),
    SizeMismatch {
        kind: CbinKind,
        expected: usize,
        actual: usize,
    },
    SchemaMismatch {
        le: u32,
        be: u16,
    },
    InvalidManifest(String),
}

impl fmt::Display for CbinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort { actual, min } => {
                write!(f, "CBIN buffer too short: {actual} bytes (minimum {min})")
            }
            Self::InvalidMagic(m) => write!(f, "invalid CBIN magic: {m:02X?}"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported CBIN format version: {v}"),
            Self::UnknownTag(t) => write!(f, "unknown CBIN tag: {t:02X?}"),
            Self::SizeMismatch {
                kind,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "{kind:?} size mismatch: expected {expected} bytes, got {actual}"
                )
            }
            Self::SchemaMismatch { le, be } => {
                write!(
                    f,
                    "schema ID mismatch between LE (0x{le:04X}) and BE (0x{be:04X})"
                )
            }
            Self::InvalidManifest(msg) => write!(f, "invalid .ns4b meta.xml: {msg}"),
        }
    }
}

impl std::error::Error for CbinError {}

/// Decoded 48-byte (`0x30`) header of a Nord Stage 4 `CBIN` preset file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CbinHeader {
    pub format_version: u32,
    pub kind: CbinKind,
    pub bank_index: u16,
    pub location_index: u16,
    pub category_id: u32,
    pub schema_id: u16,
    pub checksum: u32,
    pub reserved: [u8; 18],
}

/// Parsed Clavia Binary (`CBIN`) preset file with lossless bit-packed payload retention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CbinFile {
    pub header: CbinHeader,
    pub payload: Vec<u8>,
}

impl CbinFile {
    /// Parses and validates a `CBIN` preset buffer (`ns4p`, `ns4l`, `ns4y`, `ns4n`, `ns4o`, `ns4t`).
    pub fn parse(bytes: &[u8]) -> Result<Self, CbinError> {
        if bytes.len() < CBIN_HEADER_SIZE {
            return Err(CbinError::TooShort {
                actual: bytes.len(),
                min: CBIN_HEADER_SIZE,
            });
        }

        let magic: [u8; 4] = bytes[0..4].try_into().unwrap();
        if magic != CBIN_MAGIC {
            return Err(CbinError::InvalidMagic(magic));
        }

        let format_version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        if format_version != 1 {
            return Err(CbinError::UnsupportedVersion(format_version));
        }

        let tag: [u8; 4] = bytes[8..12].try_into().unwrap();
        let kind = CbinKind::from_tag(&tag).ok_or(CbinError::UnknownTag(tag))?;

        if let Some(expected_len) = kind.expected_file_size() {
            if bytes.len() != expected_len {
                return Err(CbinError::SizeMismatch {
                    kind,
                    expected: expected_len,
                    actual: bytes.len(),
                });
            }
        }

        let bank_index = u16::from_le_bytes(bytes[12..14].try_into().unwrap());
        let location_index = u16::from_le_bytes(bytes[14..16].try_into().unwrap());
        let category_id = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
        let schema_id_le = u32::from_le_bytes(bytes[20..24].try_into().unwrap());
        let checksum = u32::from_le_bytes(bytes[24..28].try_into().unwrap());
        let reserved: [u8; 18] = bytes[28..46].try_into().unwrap();
        let schema_id_be = u16::from_be_bytes(bytes[46..48].try_into().unwrap());

        if kind.expected_schema_id().is_some() && schema_id_le != u32::from(schema_id_be) {
            return Err(CbinError::SchemaMismatch {
                le: schema_id_le,
                be: schema_id_be,
            });
        }

        Ok(Self {
            header: CbinHeader {
                format_version,
                kind,
                bank_index,
                location_index,
                category_id,
                schema_id: schema_id_be,
                checksum,
                reserved,
            },
            payload: bytes[CBIN_HEADER_SIZE..].to_vec(),
        })
    }

    /// Serializes the `CbinFile` back into its exact binary representation.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(CBIN_HEADER_SIZE + self.payload.len());
        out.extend_from_slice(&CBIN_MAGIC);
        out.extend_from_slice(&self.header.format_version.to_le_bytes());
        out.extend_from_slice(&self.header.kind.tag());
        out.extend_from_slice(&self.header.bank_index.to_le_bytes());
        out.extend_from_slice(&self.header.location_index.to_le_bytes());
        out.extend_from_slice(&self.header.category_id.to_le_bytes());
        out.extend_from_slice(&u32::from(self.header.schema_id).to_le_bytes());
        out.extend_from_slice(&self.header.checksum.to_le_bytes());
        out.extend_from_slice(&self.header.reserved);
        out.extend_from_slice(&self.header.schema_id.to_be_bytes());
        out.extend_from_slice(&self.payload);
        out
    }

    /// For a Program (`.ns4p`) or Live (`.ns4l`) file, returns the embedded 135-byte Organ bitstream (`0x5D..0xE4`).
    pub fn embedded_organ_slice(&self) -> Option<&[u8]> {
        match self.header.kind {
            CbinKind::Program | CbinKind::Live if self.payload.len() >= 180 => {
                Some(&self.payload[45..180])
            }
            CbinKind::OrganPreset if self.payload.len() == 135 => Some(&self.payload),
            _ => None,
        }
    }

    /// For a Program (`.ns4p`) or Live (`.ns4l`) file, returns the embedded 147-byte Piano bitstream (`0xE4..0x177`).
    pub fn embedded_piano_slice(&self) -> Option<&[u8]> {
        match self.header.kind {
            CbinKind::Program | CbinKind::Live if self.payload.len() >= 327 => {
                Some(&self.payload[180..327])
            }
            CbinKind::PianoPreset if self.payload.len() == 147 => Some(&self.payload),
            _ => None,
        }
    }

    /// For a Program (`.ns4p`) or Live (`.ns4l`) file, returns the embedded 493-byte Synth bitstream (`0x177..0x364`).
    pub fn embedded_synth_slice(&self) -> Option<&[u8]> {
        match self.header.kind {
            CbinKind::Program | CbinKind::Live if self.payload.len() >= 820 => {
                Some(&self.payload[327..820])
            }
            CbinKind::SynthPreset if self.payload.len() == 493 => Some(&self.payload),
            _ => None,
        }
    }
}

/// Parsed `meta.xml` backup manifest from a `.ns4b` archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupManifest {
    pub file_version: u32,
    pub backup_format_version: u32,
    pub product_id: u32,
    pub product_version: u32,
    pub product_build: u32,
    pub manager_version: u32,
    pub manager_build: u32,
}

impl BackupManifest {
    /// Parses the root `<backup .../>` XML manifest from a `.ns4b` archive.
    pub fn parse_xml(xml: &str) -> Result<Self, CbinError> {
        let extract =
            |attr: &str| -> Result<u32, CbinError> {
                let pattern = format!("{attr}=\"");
                let start = xml.find(&pattern).ok_or_else(|| {
                    CbinError::InvalidManifest(format!("missing attribute {attr}"))
                })? + pattern.len();
                let end = xml[start..].find('"').ok_or_else(|| {
                    CbinError::InvalidManifest(format!("unclosed attribute {attr}"))
                })? + start;
                xml[start..end].parse::<u32>().map_err(|_| {
                    CbinError::InvalidManifest(format!(
                        "non-integer attribute {attr}={}",
                        &xml[start..end]
                    ))
                })
            };

        Ok(Self {
            file_version: extract("file_version")?,
            backup_format_version: extract("backup_format_version")?,
            product_id: extract("product_id")?,
            product_version: extract("product_version")?,
            product_build: extract("product_build")?,
            manager_version: extract("manager_version")?,
            manager_build: extract("manager_build")?,
        })
    }

    /// Returns `true` if this backup belongs to a Nord Stage 4 (`product_id == 46`).
    pub fn is_nord_stage_4(&self) -> bool {
        self.product_id == NS4_PRODUCT_ID
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_synthetic_cbin(kind: CbinKind, bank: u16, loc: u16, cat: u32) -> Vec<u8> {
        let total_size = kind.expected_file_size().unwrap_or(128);
        let schema = kind.expected_schema_id().unwrap_or(0x0001);
        let mut payload = vec![0xAA; total_size - CBIN_HEADER_SIZE];
        if matches!(kind, CbinKind::Program | CbinKind::Live) {
            // Embed distinct markers at Organ (45), Piano (180), and Synth (327)
            payload[45] = 0x11;
            payload[180] = 0x22;
            payload[327] = 0x33;
        }
        let file = CbinFile {
            header: CbinHeader {
                format_version: 1,
                kind,
                bank_index: bank,
                location_index: loc,
                category_id: cat,
                schema_id: schema,
                checksum: 0xCFB1_1E04,
                reserved: [0u8; 18],
            },
            payload,
        };
        file.to_bytes()
    }

    #[test]
    fn test_round_trip_all_preset_kinds() {
        for kind in [
            CbinKind::Program,
            CbinKind::Live,
            CbinKind::SynthPreset,
            CbinKind::PianoPreset,
            CbinKind::OrganPreset,
            CbinKind::Settings,
        ] {
            let raw = make_synthetic_cbin(kind, 2, 17, 0x2D);
            assert_eq!(Some(raw.len()), kind.expected_file_size());
            let parsed = CbinFile::parse(&raw).expect("valid synthetic CBIN should parse");
            assert_eq!(parsed.header.kind, kind);
            assert_eq!(parsed.header.bank_index, 2);
            assert_eq!(parsed.header.location_index, 17);
            assert_eq!(parsed.header.category_id, 0x2D);
            assert_eq!(Some(parsed.header.schema_id), kind.expected_schema_id());
            assert_eq!(parsed.to_bytes(), raw);
        }
    }

    #[test]
    fn test_embedded_sub_preset_slices_in_ns4p() {
        let raw = make_synthetic_cbin(CbinKind::Program, 0, 0, 0x12);
        let parsed = CbinFile::parse(&raw).unwrap();
        let organ = parsed.embedded_organ_slice().unwrap();
        let piano = parsed.embedded_piano_slice().unwrap();
        let synth = parsed.embedded_synth_slice().unwrap();
        assert_eq!(organ.len(), 135);
        assert_eq!(organ[0], 0x11);
        assert_eq!(piano.len(), 147);
        assert_eq!(piano[0], 0x22);
        assert_eq!(synth.len(), 493);
        assert_eq!(synth[0], 0x33);
    }

    #[test]
    fn test_rejects_corrupted_headers() {
        assert!(matches!(
            CbinFile::parse(&[0u8; 12]),
            Err(CbinError::TooShort {
                actual: 12,
                min: 48
            })
        ));

        let mut bad_magic = make_synthetic_cbin(CbinKind::OrganPreset, 0, 0, 0);
        bad_magic[0] = b'X';
        assert!(matches!(
            CbinFile::parse(&bad_magic),
            Err(CbinError::InvalidMagic(_))
        ));

        let mut bad_schema = make_synthetic_cbin(CbinKind::OrganPreset, 0, 0, 0);
        bad_schema[47] ^= 0xFF;
        assert!(matches!(
            CbinFile::parse(&bad_schema),
            Err(CbinError::SchemaMismatch { .. })
        ));
    }

    #[test]
    fn test_parse_ns4b_meta_xml() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<backup file_version="1" backup_format_version="1" product_id="46" product_version="166" product_build="2934" product_content_version="0" manager_version="918" manager_build="1444"/>"#;
        let manifest = BackupManifest::parse_xml(xml).expect("valid factory restore meta.xml");
        assert!(manifest.is_nord_stage_4());
        assert_eq!(manifest.product_version, 166);
        assert_eq!(manifest.product_build, 2934);
        assert_eq!(manifest.manager_version, 918);
    }
}
