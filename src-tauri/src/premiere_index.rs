use flate2::read::GzDecoder;
use quick_xml::{events::Event, Reader};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::{Cursor, Read},
    path::Path,
    time::UNIX_EPOCH,
};

pub const PARSER_VERSION: &str = "pimp-prproj-1";
const MAX_COMPRESSED_BYTES: u64 = 256 * 1024 * 1024;
const MAX_DECOMPRESSED_BYTES: u64 = 512 * 1024 * 1024;
const MAX_XML_EVENTS: usize = 2_000_000;
const MAX_XML_DEPTH: usize = 256;
const MAX_VALUE_BYTES: usize = 32 * 1024;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PremiereMediaReference {
    pub path: String,
    pub evidence: String,
    pub media_id: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PremiereSequence {
    pub id: String,
    pub name: String,
    pub nested: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PremiereProjectIndex {
    pub parser_version: String,
    pub source_relative_path: String,
    pub source_fingerprint: String,
    pub source_modified_millis: u64,
    pub indexed_at: String,
    pub status: String,
    pub format_version: Option<String>,
    pub media_references: Vec<PremiereMediaReference>,
    pub sequences: Vec<PremiereSequence>,
    pub bins: Vec<String>,
    pub issues: Vec<String>,
}

fn bounded_project_bytes(path: &Path) -> Result<(Vec<u8>, u64), String> {
    let before = fs::metadata(path)
        .map_err(|error| format!("Cannot inspect saved Premiere project: {error}"))?;
    if before.len() > MAX_COMPRESSED_BYTES {
        return Err(format!(
            "Saved Premiere project exceeds the {} MiB compressed safety limit",
            MAX_COMPRESSED_BYTES / 1024 / 1024
        ));
    }
    let modified = before
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or_default();
    let bytes = fs::read(path)
        .map_err(|error| format!("Cannot read stable Premiere project snapshot: {error}"))?;
    let after = fs::metadata(path)
        .map_err(|error| format!("Cannot recheck saved Premiere project: {error}"))?;
    if before.len() != after.len() || before.modified().ok() != after.modified().ok() {
        return Err(
            "Premiere project changed while it was being read; save fully and retry".into(),
        );
    }
    let expected = Sha256::digest(&bytes);
    let mut stable_read = fs::File::open(path)
        .map_err(|error| format!("Cannot reopen Premiere project for stability check: {error}"))?;
    let mut actual = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let read = stable_read
            .read(&mut buffer)
            .map_err(|error| format!("Cannot verify stable Premiere project read: {error}"))?;
        if read == 0 {
            break;
        }
        actual.update(&buffer[..read]);
    }
    if actual.finalize().as_slice() != expected.as_slice() {
        return Err(
            "Premiere project changed while it was being read; save fully and retry".into(),
        );
    }
    Ok((bytes, modified))
}

fn decoded_xml(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let reader: Box<dyn Read> = if bytes.starts_with(&[0x1f, 0x8b]) {
        Box::new(GzDecoder::new(Cursor::new(bytes)))
    } else if bytes.iter().take(128).any(|byte| *byte == b'<') {
        Box::new(Cursor::new(bytes))
    } else {
        return Err("Unsupported Premiere project container (expected gzip XML or XML)".into());
    };
    let mut output = Vec::new();
    reader
        .take(MAX_DECOMPRESSED_BYTES + 1)
        .read_to_end(&mut output)
        .map_err(|error| format!("Cannot decompress Premiere project safely: {error}"))?;
    if output.len() as u64 > MAX_DECOMPRESSED_BYTES {
        return Err(format!(
            "Premiere project exceeds the {} MiB decompressed safety limit",
            MAX_DECOMPRESSED_BYTES / 1024 / 1024
        ));
    }
    Ok(output)
}

fn attribute_value(event: &quick_xml::events::BytesStart<'_>, names: &[&str]) -> Option<String> {
    event
        .attributes()
        .with_checks(true)
        .flatten()
        .find_map(|attr| {
            let name = attr.key.as_ref();
            names
                .iter()
                .any(|candidate| name.eq_ignore_ascii_case(candidate))
                .then(|| {
                    attr.value
                        .as_ref()
                        .trim()
                        .chars()
                        .take(MAX_VALUE_BYTES)
                        .collect()
                })
        })
}

fn parse_xml(
    xml: &[u8],
) -> Result<
    (
        Option<String>,
        Vec<String>,
        Vec<PremiereSequence>,
        Vec<String>,
        Vec<String>,
    ),
    String,
> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    let mut depth = 0usize;
    let mut events = 0usize;
    let mut root_seen = false;
    let mut root_closed = false;
    let mut version = None;
    let mut capture_path = false;
    let mut paths = BTreeSet::new();
    let mut sequences = Vec::new();
    let mut bins = BTreeSet::new();
    let mut issues = Vec::new();
    loop {
        events += 1;
        if events > MAX_XML_EVENTS {
            return Err("Premiere XML exceeds the bounded event limit".into());
        }
        match reader.read_event() {
            Ok(event @ (Event::Start(_) | Event::Empty(_))) => {
                let empty = matches!(event, Event::Empty(_));
                let event = match &event {
                    Event::Start(event) | Event::Empty(event) => event,
                    _ => unreachable!(),
                };
                depth += 1;
                if depth > MAX_XML_DEPTH {
                    return Err("Premiere XML nesting exceeds the safety limit".into());
                }
                let name = event.name().as_ref().to_string();
                if root_closed {
                    return Err("Malformed Premiere XML: multiple root elements".into());
                }
                if !root_seen {
                    root_seen = true;
                    if name != "PremiereData" && name != "Project" {
                        return Err(format!("Unsupported Premiere XML root element: {name}"));
                    }
                    version = attribute_value(&event, &["Version", "version"]);
                }
                if ["ActualMediaFilePath", "FilePath", "MediaPath"]
                    .iter()
                    .any(|candidate| name.eq_ignore_ascii_case(candidate))
                {
                    capture_path = true;
                }
                if name.eq_ignore_ascii_case("MediaRef") || name.eq_ignore_ascii_case("ProjectItem")
                {
                    if let Some(path) = attribute_value(&event, &["Path", "FilePath", "MediaPath"])
                    {
                        if !path.is_empty() {
                            paths.insert(path);
                        }
                    }
                }
                if name.eq_ignore_ascii_case("Sequence")
                    || name.eq_ignore_ascii_case("NestedSequence")
                {
                    let id = attribute_value(&event, &["ObjectID", "ObjectId", "id"])
                        .unwrap_or_else(|| format!("sequence-{}", sequences.len() + 1));
                    let sequence_name = attribute_value(&event, &["Name", "name"])
                        .unwrap_or_else(|| "Unnamed sequence".into());
                    let nested = attribute_value(&event, &["Nested", "nested"])
                        .is_some_and(|value| value.eq_ignore_ascii_case("true") || value == "1");
                    sequences.push(PremiereSequence {
                        id,
                        name: sequence_name,
                        nested,
                    });
                } else if name.eq_ignore_ascii_case("Bin") {
                    if let Some(name) = attribute_value(&event, &["Name", "name"]) {
                        if !name.is_empty() {
                            bins.insert(name);
                        }
                    }
                }
                if empty {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        root_closed = true;
                    }
                }
            }
            Ok(Event::Text(text)) => {
                if capture_path {
                    let path = text.as_ref();
                    let path = path.trim();
                    if !path.is_empty() && path.len() <= MAX_VALUE_BYTES {
                        paths.insert(path.into());
                    }
                    capture_path = false;
                }
            }
            Ok(Event::End(_)) => {
                capture_path = false;
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    root_closed = true;
                }
            }
            Ok(Event::DocType(_)) => {
                return Err("Premiere XML document types are not accepted".into())
            }
            Ok(Event::Eof) => {
                if depth != 0 {
                    return Err("Malformed Premiere XML: unclosed elements".into());
                }
                if !root_seen {
                    return Err("Malformed Premiere XML: no root element".into());
                }
                break;
            }
            Err(error) => return Err(format!("Malformed Premiere XML: {error}")),
            _ => {}
        }
    }
    if sequences.is_empty() {
        issues.push("No structurally recognized sequences were found".into());
    }
    Ok((
        version,
        paths.into_iter().collect(),
        sequences,
        bins.into_iter().collect(),
        issues,
    ))
}

pub fn index_project(
    path: &Path,
    relative_path: &str,
    indexed_at: String,
) -> Result<PremiereProjectIndex, String> {
    let (compressed, modified) = bounded_project_bytes(path)?;
    let fingerprint = format!("{:x}", Sha256::digest(&compressed));
    let xml = decoded_xml(&compressed)?;
    let (version, paths, sequences, bins, mut issues) = parse_xml(&xml)?;
    let status = if paths.is_empty() {
        issues.push("No structurally recognized imported-media paths were found".into());
        "partial"
    } else {
        "supported"
    };
    Ok(PremiereProjectIndex {
        parser_version: PARSER_VERSION.into(),
        source_relative_path: relative_path.into(),
        source_fingerprint: fingerprint,
        source_modified_millis: modified,
        indexed_at,
        status: status.into(),
        format_version: version,
        media_references: paths
            .into_iter()
            .map(|path| PremiereMediaReference {
                path,
                evidence: "imported-into-saved-project".into(),
                media_id: None,
            })
            .collect(),
        sequences,
        bins,
        issues,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn indexes_bounded_gzip_xml_without_modifying_the_project() {
        let root = std::env::temp_dir().join(format!("pimp-prproj-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("Proof.prproj");
        let xml = br#"<PremiereData Version="42"><Project><Bin Name="Media"/><ProjectItem ObjectID="i1"><ActualMediaFilePath>C:\Master\clip.mp4</ActualMediaFilePath></ProjectItem><Sequence ObjectID="s1" Name="Main"/><Sequence ObjectID="s2" Name="Nested" Nested="true"/></Project></PremiereData>"#;
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(xml).unwrap();
        let bytes = encoder.finish().unwrap();
        fs::write(&path, &bytes).unwrap();
        let index = index_project(&path, "Proof.prproj", "now".into()).unwrap();
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(index.status, "supported");
        assert_eq!(index.media_references[0].path, r"C:\Master\clip.mp4");
        assert_eq!(index.sequences.len(), 2);
        assert!(index.sequences[1].nested);
        assert_eq!(index.bins, vec!["Media"]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_doctypes_malformed_xml_and_unknown_containers() {
        assert!(parse_xml(b"<!DOCTYPE x><PremiereData/>").is_err());
        assert!(parse_xml(b"<PremiereData><Sequence>").is_err());
        assert!(parse_xml(b"<PremiereData/><PremiereData/>").is_err());
        assert!(parse_xml(b"").is_err());
        assert!(decoded_xml(b"not a project").is_err());
    }
}
