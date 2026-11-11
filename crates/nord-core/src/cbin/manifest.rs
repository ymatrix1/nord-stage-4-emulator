//! Parsed `meta.xml` backup manifest from a `.ns4b` archive.

use super::container::{CbinError, NS4_PRODUCT_ID};

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

    #[test]
    fn test_parse_ns4b_meta_xml() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<backup file_version="1" backup_format_version="1" product_id="46" product_version="166" product_build="2934" product_content_version="0" manager_version="918" manager_build="1444"/>"#;
        let manifest = BackupManifest::parse_xml(xml).expect("valid factory restore meta.xml");
        assert!(manifest.is_nord_stage_4());
        assert_eq!(manifest.product_version, 166);
        assert_eq!(manifest.product_build, 2934);
        assert_eq!(manifest.manager_version, 918);
        assert_eq!(manifest.manager_build, 1444);
    }

    #[test]
    fn test_invalid_manifest() {
        assert!(BackupManifest::parse_xml("<invalid/>").is_err());
        assert!(BackupManifest::parse_xml(r#"<backup file_version="abc"/>"#).is_err());
    }
}
