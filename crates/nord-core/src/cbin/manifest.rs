//! Parsed `meta.xml` backup manifest from a `.ns4b` archive.

use super::container::{CbinError, NS4_PRODUCT_ID};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

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
        let mut reader = Reader::from_str(xml);
        let mut fields = [None; 7];
        let mut root_seen = false;
        let mut root_closed = false;
        let mut depth = 0usize;

        loop {
            let event = reader
                .read_event()
                .map_err(|error| CbinError::InvalidManifest(format!("malformed XML: {error}")))?;
            match event {
                Event::Start(start) if !root_seen => {
                    if start.name().as_ref() != b"backup" {
                        return Err(CbinError::InvalidManifest(
                            "root element must be <backup>".to_owned(),
                        ));
                    }
                    parse_manifest_attributes(&start, &mut fields)?;
                    root_seen = true;
                    depth = 1;
                }
                Event::Empty(start) if !root_seen => {
                    if start.name().as_ref() != b"backup" {
                        return Err(CbinError::InvalidManifest(
                            "root element must be <backup>".to_owned(),
                        ));
                    }
                    parse_manifest_attributes(&start, &mut fields)?;
                    root_seen = true;
                    root_closed = true;
                }
                Event::Start(_) if root_seen && !root_closed => depth += 1,
                Event::Empty(_) if root_seen && !root_closed => {}
                Event::End(_) if root_seen && !root_closed => {
                    depth = depth.saturating_sub(1);
                    root_closed = depth == 0;
                }
                Event::Eof => break,
                Event::Text(text)
                    if !root_seen && text.decode().is_ok_and(|value| value.trim().is_empty()) => {}
                Event::Text(text)
                    if root_closed && text.decode().is_ok_and(|value| value.trim().is_empty()) => {}
                Event::Decl(_) | Event::Comment(_) if !root_seen => {}
                Event::Comment(_) | Event::PI(_) if root_closed => {}
                _ if root_closed => {
                    return Err(CbinError::InvalidManifest(
                        "content found after </backup>".to_owned(),
                    ));
                }
                _ => {}
            }
        }

        if !root_seen || !root_closed {
            return Err(CbinError::InvalidManifest(
                "missing or incomplete <backup> root element".to_owned(),
            ));
        }

        let [file_version, backup_format_version, product_id, product_version, product_build, manager_version, manager_build] =
            fields;
        Ok(Self {
            file_version: required_field(file_version, "file_version")?,
            backup_format_version: required_field(backup_format_version, "backup_format_version")?,
            product_id: required_field(product_id, "product_id")?,
            product_version: required_field(product_version, "product_version")?,
            product_build: required_field(product_build, "product_build")?,
            manager_version: required_field(manager_version, "manager_version")?,
            manager_build: required_field(manager_build, "manager_build")?,
        })
    }

    /// Returns `true` if this backup belongs to a Nord Stage 4 (`product_id == 46`).
    pub fn is_nord_stage_4(&self) -> bool {
        self.product_id == NS4_PRODUCT_ID
    }
}

fn parse_manifest_attributes(
    start: &BytesStart<'_>,
    fields: &mut [Option<u32>; 7],
) -> Result<(), CbinError> {
    for attribute in start.attributes().with_checks(true) {
        let attribute = attribute.map_err(|error| {
            CbinError::InvalidManifest(format!("malformed XML attribute: {error}"))
        })?;
        let field_index = match attribute.key.as_ref() {
            b"file_version" => 0,
            b"backup_format_version" => 1,
            b"product_id" => 2,
            b"product_version" => 3,
            b"product_build" => 4,
            b"manager_version" => 5,
            b"manager_build" => 6,
            _ => continue,
        };
        if fields[field_index].is_some() {
            return Err(CbinError::InvalidManifest(format!(
                "duplicate attribute {}",
                String::from_utf8_lossy(attribute.key.as_ref())
            )));
        }
        let value = attribute
            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
            .map_err(|error| {
                CbinError::InvalidManifest(format!("invalid XML attribute value: {error}"))
            })?;
        fields[field_index] = Some(value.parse::<u32>().map_err(|_| {
            CbinError::InvalidManifest(format!(
                "non-integer attribute {}={value}",
                String::from_utf8_lossy(attribute.key.as_ref())
            ))
        })?);
    }
    Ok(())
}

fn required_field(value: Option<u32>, name: &str) -> Result<u32, CbinError> {
    value.ok_or_else(|| CbinError::InvalidManifest(format!("missing attribute {name}")))
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

    #[test]
    fn test_parse_xml_attribute_spacing_and_single_quotes() {
        let xml = "<?xml version='1.0'?>\n<backup\n file_version = '1'\n backup_format_version='1'\n product_id = '46'\n product_version='166'\n product_build = '2934'\n manager_version='918'\n manager_build = '1444' />\n";
        let manifest = BackupManifest::parse_xml(xml).unwrap();
        assert_eq!(manifest.file_version, 1);
        assert_eq!(manifest.product_id, NS4_PRODUCT_ID);
        assert_eq!(manifest.manager_build, 1444);
    }

    #[test]
    fn test_manifest_requires_backup_root_and_complete_well_formed_xml() {
        let fields = "file_version='1' backup_format_version='1' product_id='46' product_version='166' product_build='2934' manager_version='918' manager_build='1444'";
        assert!(BackupManifest::parse_xml(&format!("<other><backup {fields}/></other>")).is_err());
        assert!(BackupManifest::parse_xml(&format!("<backup {fields}>")).is_err());
        assert!(
            BackupManifest::parse_xml(&format!("<backup {fields}/><backup {fields}/>")).is_err()
        );
        assert!(BackupManifest::parse_xml(&format!("<backup {fields}></wrong>")).is_err());
    }
}
