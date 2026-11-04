use flate2::read::GzDecoder;
use quick_xml::{events::Event, Reader};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
    fs,
    io::{Cursor, Read},
    path::Path,
    time::UNIX_EPOCH,
};

pub const PARSER_VERSION: &str = "pimp-prproj-2";
const MAX_COMPRESSED_BYTES: u64 = 256 * 1024 * 1024;
const MAX_DECOMPRESSED_BYTES: u64 = 512 * 1024 * 1024;
const MAX_XML_EVENTS: usize = 2_000_000;
const MAX_XML_DEPTH: usize = 256;
const MAX_VALUE_BYTES: usize = 32 * 1024;
const MAX_GRAPH_VISITS: usize = 20_000;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PremiereMediaReference {
    pub path: String,
    pub evidence: String,
    pub media_id: Option<String>,
    #[serde(default)]
    pub sequence_ids: Vec<String>,
    #[serde(default)]
    pub timeline_placements: u64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PremiereSequence {
    pub id: String,
    pub name: String,
    pub nested: bool,
    #[serde(default)]
    pub media_paths: Vec<String>,
    #[serde(default)]
    pub timeline_placements: u64,
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

#[derive(Default)]
struct Record {
    tag: String,
    uid: Option<String>,
    id: Option<String>,
    refs: BTreeSet<String>,
    children: Vec<usize>,
    parent: Option<usize>,
    name: Option<String>,
    paths: BTreeSet<String>,
}

struct Open {
    tag: String,
    owner: Option<usize>,
}

#[derive(Default)]
struct Graph {
    version: Option<String>,
    records: Vec<Record>,
}

fn bounded_project_bytes(path: &Path) -> Result<(Vec<u8>, u64), String> {
    let before =
        fs::metadata(path).map_err(|e| format!("Cannot inspect saved Premiere project: {e}"))?;
    if before.len() > MAX_COMPRESSED_BYTES {
        return Err(format!(
            "Saved Premiere project exceeds the {} MiB compressed safety limit",
            MAX_COMPRESSED_BYTES / 1024 / 1024
        ));
    }
    let modified = before
        .modified()
        .ok()
        .and_then(|v| v.duration_since(UNIX_EPOCH).ok())
        .map(|v| v.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or_default();
    let bytes =
        fs::read(path).map_err(|e| format!("Cannot read stable Premiere project snapshot: {e}"))?;
    let after =
        fs::metadata(path).map_err(|e| format!("Cannot recheck saved Premiere project: {e}"))?;
    if before.len() != after.len() || before.modified().ok() != after.modified().ok() {
        return Err(
            "Premiere project changed while it was being read; save fully and retry".into(),
        );
    }
    let expected = Sha256::digest(&bytes);
    let mut input = fs::File::open(path)
        .map_err(|e| format!("Cannot reopen Premiere project for stability check: {e}"))?;
    let mut actual = Sha256::new();
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        let count = input
            .read(&mut buffer)
            .map_err(|e| format!("Cannot verify stable Premiere project read: {e}"))?;
        if count == 0 {
            break;
        }
        actual.update(&buffer[..count]);
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
    } else if bytes.iter().take(128).any(|v| *v == b'<') {
        Box::new(Cursor::new(bytes))
    } else {
        return Err("Unsupported Premiere project container (expected gzip XML or XML)".into());
    };
    let mut output = Vec::new();
    reader
        .take(MAX_DECOMPRESSED_BYTES + 1)
        .read_to_end(&mut output)
        .map_err(|e| format!("Cannot decompress Premiere project safely: {e}"))?;
    if output.len() as u64 > MAX_DECOMPRESSED_BYTES {
        return Err(format!(
            "Premiere project exceeds the {} MiB decompressed safety limit",
            MAX_DECOMPRESSED_BYTES / 1024 / 1024
        ));
    }
    Ok(output)
}

fn attr(event: &quick_xml::events::BytesStart<'_>, name: &str) -> Option<String> {
    event
        .attributes()
        .with_checks(true)
        .flatten()
        .find(|v| v.key.as_ref().eq_ignore_ascii_case(name))
        .map(|v| {
            v.value
                .as_ref()
                .trim()
                .chars()
                .take(MAX_VALUE_BYTES)
                .collect()
        })
        .filter(|v: &String| !v.is_empty())
}

fn structural(tag: &str) -> bool {
    [
        "Sequence",
        "ClipProjectItem",
        "BinProjectItem",
        "ProjectItemContainer",
        "VideoClipTrackItem",
        "AudioClipTrackItem",
        "Clip",
        "Media",
        "ProjectItem",
    ]
    .iter()
    .any(|v| tag.eq_ignore_ascii_case(v))
}

fn parse_graph(xml: &[u8]) -> Result<Graph, String> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    let mut graph = Graph::default();
    let mut stack: Vec<Open> = Vec::new();
    let mut event_count = 0;
    let mut root_seen = false;
    let mut root_closed = false;
    loop {
        event_count += 1;
        if event_count > MAX_XML_EVENTS {
            return Err("Premiere XML exceeds the bounded event limit".into());
        }
        match reader.read_event() {
            Ok(event @ (Event::Start(_) | Event::Empty(_))) => {
                let empty = matches!(event, Event::Empty(_));
                let event = match &event {
                    Event::Start(v) | Event::Empty(v) => v,
                    _ => unreachable!(),
                };
                if stack.len() >= MAX_XML_DEPTH {
                    return Err("Premiere XML nesting exceeds the safety limit".into());
                }
                let tag = event.name().as_ref().to_string();
                if root_closed {
                    return Err("Malformed Premiere XML: multiple root elements".into());
                }
                if !root_seen {
                    root_seen = true;
                    if tag != "PremiereData" && tag != "Project" {
                        return Err(format!("Unsupported Premiere XML root element: {tag}"));
                    }
                    graph.version = attr(event, "Version");
                }
                let uid = attr(event, "ObjectUID");
                let id = attr(event, "ObjectID");
                let parent = stack.last().and_then(|v| v.owner);
                let owner = if structural(&tag) || uid.is_some() || id.is_some() {
                    let index = graph.records.len();
                    graph.records.push(Record {
                        tag: tag.clone(),
                        uid,
                        id,
                        parent,
                        ..Default::default()
                    });
                    if let Some(parent) = parent {
                        graph.records[parent].children.push(index);
                    }
                    Some(index)
                } else {
                    parent
                };
                if let Some(owner) = owner {
                    for name in ["ObjectURef", "ObjectRef"] {
                        if let Some(value) = attr(event, name) {
                            graph.records[owner].refs.insert(value);
                        }
                    }
                    for name in ["Path", "FilePath", "MediaPath"] {
                        if let Some(value) = attr(event, name) {
                            graph.records[owner].paths.insert(value);
                        }
                    }
                }
                stack.push(Open { tag, owner });
                if empty {
                    stack.pop();
                    if stack.is_empty() {
                        root_closed = true;
                    }
                }
            }
            Ok(Event::Text(text)) => {
                let Some(open) = stack.last() else { continue };
                let Some(owner) = open.owner else { continue };
                let value = text.as_ref().trim().to_string();
                if value.is_empty() || value.len() > MAX_VALUE_BYTES {
                    continue;
                }
                if open.tag.eq_ignore_ascii_case("Name") && graph.records[owner].name.is_none() {
                    graph.records[owner].name = Some(value);
                } else if ["ActualMediaFilePath", "FilePath", "MediaPath"]
                    .iter()
                    .any(|v| open.tag.eq_ignore_ascii_case(v))
                {
                    graph.records[owner].paths.insert(value);
                }
            }
            Ok(Event::End(_)) => {
                if stack.pop().is_none() {
                    return Err("Malformed Premiere XML: unexpected closing element".into());
                }
                if stack.is_empty() {
                    root_closed = true;
                }
            }
            Ok(Event::DocType(_)) => {
                return Err("Premiere XML document types are not accepted".into())
            }
            Ok(Event::Eof) => {
                if !stack.is_empty() {
                    return Err("Malformed Premiere XML: unclosed elements".into());
                }
                if !root_seen {
                    return Err("Malformed Premiere XML: no root element".into());
                }
                break;
            }
            Err(e) => return Err(format!("Malformed Premiere XML: {e}")),
            _ => {}
        }
    }
    Ok(graph)
}

fn definitions(records: &[Record]) -> HashMap<String, Vec<usize>> {
    let mut result: HashMap<String, Vec<usize>> = HashMap::new();
    for (index, record) in records.iter().enumerate() {
        for id in [record.uid.as_ref(), record.id.as_ref()]
            .into_iter()
            .flatten()
        {
            result.entry(id.clone()).or_default().push(index);
        }
    }
    result
}

fn targets(index: usize, records: &[Record], defs: &HashMap<String, Vec<usize>>) -> Vec<usize> {
    let mut result = records[index].children.clone();
    for reference in &records[index].refs {
        if let Some(found) = defs.get(reference) {
            if found.len() == 1 {
                result.push(found[0]);
            }
        }
    }
    result.sort_unstable();
    result.dedup();
    result
}

fn container_items(
    index: usize,
    records: &[Record],
    defs: &HashMap<String, Vec<usize>>,
) -> Vec<usize> {
    let mut result = Vec::new();
    for child in &records[index].children {
        if records[*child].uid.is_some() || records[*child].id.is_some() {
            result.push(*child);
        }
        for reference in &records[*child].refs {
            if let Some(found) = defs.get(reference) {
                if found.len() == 1 {
                    result.push(found[0]);
                }
            }
        }
    }
    for reference in &records[index].refs {
        if let Some(found) = defs.get(reference) {
            if found.len() == 1 {
                result.push(found[0]);
            }
        }
    }
    result.sort_unstable();
    result.dedup();
    result
}

fn reachable_paths(
    start: usize,
    records: &[Record],
    defs: &HashMap<String, Vec<usize>>,
) -> BTreeSet<String> {
    let mut result = BTreeSet::new();
    let mut queue = VecDeque::from([start]);
    let mut seen = HashSet::new();
    while let Some(index) = queue.pop_front() {
        if !seen.insert(index) || seen.len() > MAX_GRAPH_VISITS {
            continue;
        }
        result.extend(records[index].paths.iter().cloned());
        queue.extend(targets(index, records, defs));
    }
    result
}

fn owning_sequence(mut index: usize, records: &[Record]) -> Option<usize> {
    loop {
        if records[index].tag.eq_ignore_ascii_case("Sequence") && records[index].uid.is_some() {
            return Some(index);
        }
        index = records[index].parent?;
    }
}

fn build_index(
    graph: Graph,
) -> (
    Vec<PremiereMediaReference>,
    Vec<PremiereSequence>,
    Vec<String>,
    Vec<String>,
) {
    let records = graph.records;
    let defs = definitions(&records);
    let sequence_indexes: Vec<_> = records
        .iter()
        .enumerate()
        .filter_map(|(i, r)| {
            (r.tag.eq_ignore_ascii_case("Sequence") && r.uid.is_some()).then_some(i)
        })
        .collect();
    let sequence_ids: HashMap<_, _> = sequence_indexes
        .iter()
        .filter_map(|i| records[*i].uid.clone().map(|id| (id, *i)))
        .collect();
    let containers: Vec<_> = records
        .iter()
        .enumerate()
        .filter_map(|(i, r)| {
            r.tag
                .eq_ignore_ascii_case("ProjectItemContainer")
                .then_some(i)
        })
        .collect();
    let contained: BTreeSet<_> = containers
        .iter()
        .flat_map(|i| {
            records[*i].refs.iter().cloned().chain(
                records[*i]
                    .children
                    .iter()
                    .flat_map(|child| records[*child].refs.iter().cloned()),
            )
        })
        .collect();
    let nested_bins: BTreeSet<_> = contained
        .iter()
        .filter(|id| {
            defs.get(*id).is_some_and(|v| {
                v.iter()
                    .any(|i| records[*i].tag.eq_ignore_ascii_case("BinProjectItem"))
            })
        })
        .cloned()
        .collect();
    let root_bins: HashSet<_> = records
        .iter()
        .enumerate()
        .filter_map(|(i, r)| {
            if !r.tag.eq_ignore_ascii_case("BinProjectItem") {
                return None;
            }
            r.uid
                .as_ref()
                .or(r.id.as_ref())
                .is_none_or(|id| !nested_bins.contains(id))
                .then_some(i)
        })
        .collect();
    let root_containers: Vec<_> = containers
        .iter()
        .copied()
        .filter(|i| {
            records[*i].parent.is_none_or(|p| {
                !records[p].tag.eq_ignore_ascii_case("BinProjectItem") || root_bins.contains(&p)
            })
        })
        .collect();
    let mut top_sequences = BTreeSet::new();
    for container in root_containers {
        for target in container_items(container, &records, &defs) {
            if !records[target].tag.eq_ignore_ascii_case("ClipProjectItem") {
                continue;
            }
            let mut queue = VecDeque::from([target]);
            let mut seen = HashSet::new();
            while let Some(i) = queue.pop_front() {
                if !seen.insert(i) || seen.len() > MAX_GRAPH_VISITS {
                    continue;
                }
                if let Some(id) = records[i]
                    .uid
                    .as_ref()
                    .filter(|id| sequence_ids.contains_key(*id))
                {
                    top_sequences.insert(id.clone());
                }
                top_sequences.extend(
                    records[i]
                        .refs
                        .iter()
                        .filter(|id| sequence_ids.contains_key(*id))
                        .cloned(),
                );
                queue.extend(
                    targets(i, &records, &defs).into_iter().filter(|child| {
                        !records[*child].tag.eq_ignore_ascii_case("BinProjectItem")
                    }),
                );
            }
        }
    }
    let mut usage: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
    for (i, record) in records.iter().enumerate() {
        if !record.tag.eq_ignore_ascii_case("VideoClipTrackItem")
            && !record.tag.eq_ignore_ascii_case("AudioClipTrackItem")
        {
            continue;
        }
        let Some(sequence) = owning_sequence(i, &records) else {
            continue;
        };
        let Some(sequence_id) = records[sequence].uid.as_ref() else {
            continue;
        };
        for path in reachable_paths(i, &records, &defs) {
            *usage
                .entry(path)
                .or_default()
                .entry(sequence_id.clone())
                .or_default() += 1;
        }
    }
    let project_items: Vec<_> = containers
        .iter()
        .flat_map(|i| container_items(*i, &records, &defs))
        .filter(|i| records[*i].tag.eq_ignore_ascii_case("ClipProjectItem"))
        .collect();
    let imported: BTreeSet<_> = project_items
        .iter()
        .flat_map(|i| reachable_paths(*i, &records, &defs))
        .collect();
    let all_paths: BTreeSet<_> = records
        .iter()
        .flat_map(|r| r.paths.iter().cloned())
        .collect();
    let media = all_paths
        .into_iter()
        .map(|path| {
            let used = usage.get(&path);
            PremiereMediaReference {
                evidence: if used.is_some() {
                    "used-in-saved-timeline"
                } else if imported.contains(&path) {
                    "imported-into-saved-project"
                } else {
                    "previously-referenced-in-saved-project"
                }
                .into(),
                sequence_ids: used
                    .map(|v| v.keys().cloned().collect())
                    .unwrap_or_default(),
                timeline_placements: used.map(|v| v.values().sum()).unwrap_or_default(),
                path,
                media_id: None,
            }
        })
        .collect();
    let mut omitted = 0;
    let mut sequences = Vec::new();
    for i in sequence_indexes {
        let id = records[i].uid.clone().unwrap();
        let Some(name) = records[i].name.clone().filter(|v| !v.is_empty()) else {
            omitted += 1;
            continue;
        };
        let mut media_paths = Vec::new();
        let mut placements = 0;
        for (path, by_sequence) in &usage {
            if let Some(count) = by_sequence.get(&id) {
                media_paths.push(path.clone());
                placements += count;
            }
        }
        sequences.push(PremiereSequence {
            nested: !top_sequences.contains(&id),
            id,
            name,
            media_paths,
            timeline_placements: placements,
        });
    }
    sequences.sort_by(|a, b| a.nested.cmp(&b.nested).then(a.name.cmp(&b.name)));
    let bins: BTreeSet<_> = records
        .iter()
        .filter(|r| r.tag.eq_ignore_ascii_case("BinProjectItem"))
        .filter_map(|r| r.name.clone())
        .collect();
    let mut issues = Vec::new();
    if sequences.is_empty() {
        issues.push("No named canonical sequence definitions were established".into());
    }
    if omitted > 0 {
        issues.push(format!(
            "{omitted} canonical sequence definition(s) had no usable child Name and were omitted"
        ));
    }
    if !sequences.is_empty() && top_sequences.is_empty() {
        issues.push(
            "Sequence definitions were found, but project-bin placement could not be established"
                .into(),
        );
    }
    if usage.is_empty() {
        issues.push(
            "Timeline track-item media relationships were not established; usage remains unknown"
                .into(),
        );
    }
    issues.push("Source and sequence time ranges are not reported because this parser version does not yet verify Premiere time-base fields".into());
    (media, sequences, bins.into_iter().collect(), issues)
}

pub fn index_project(
    path: &Path,
    relative_path: &str,
    indexed_at: String,
) -> Result<PremiereProjectIndex, String> {
    let (compressed, modified) = bounded_project_bytes(path)?;
    let fingerprint = format!("{:x}", Sha256::digest(&compressed));
    let graph = parse_graph(&decoded_xml(&compressed)?)?;
    let version = graph.version.clone();
    let (media_references, sequences, bins, mut issues) = build_index(graph);
    if media_references.is_empty() {
        issues.push("No structurally recognized media-path evidence was found".into());
    }
    Ok(PremiereProjectIndex {
        parser_version: PARSER_VERSION.into(),
        source_relative_path: relative_path.into(),
        source_fingerprint: fingerprint,
        source_modified_millis: modified,
        indexed_at,
        status: if media_references.is_empty() || sequences.is_empty() || !issues.is_empty() {
            "partial"
        } else {
            "supported"
        }
        .into(),
        format_version: version,
        media_references,
        sequences,
        bins,
        issues,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    const FIXTURE: &[u8] = br#"<PremiereData Version="3"><Project>
      <BinProjectItem ObjectUID="root"><Name>Root</Name><ProjectItemContainer><ClipProjectItem ObjectRef="p1"/><ClipProjectItem ObjectRef="p2"/><BinProjectItem ObjectRef="nested"/></ProjectItemContainer></BinProjectItem>
      <BinProjectItem ObjectUID="nested"><Name>Nested</Name><ProjectItemContainer><ClipProjectItem ObjectRef="p3"/><ClipProjectItem ObjectRef="p4"/></ProjectItemContainer></BinProjectItem>
      <ClipProjectItem ObjectID="p1"><Sequence ObjectURef="s1"/></ClipProjectItem><ClipProjectItem ObjectID="p2"><Sequence ObjectURef="s2"/></ClipProjectItem>
      <ClipProjectItem ObjectID="p3"><Sequence ObjectURef="s3"/></ClipProjectItem><ClipProjectItem ObjectID="p4"><Sequence ObjectURef="s4"/></ClipProjectItem>
      <ProjectItemContainer><ClipProjectItem ObjectRef="pm"/></ProjectItemContainer><ClipProjectItem ObjectID="pm"><Media ObjectRef="m1"/></ClipProjectItem>
      <Media ObjectID="m1"><ActualMediaFilePath>C:\Master\current.mp4</ActualMediaFilePath></Media><Media ObjectID="old"><ActualMediaFilePath>D:\Old\previous.mp4</ActualMediaFilePath></Media>
      <Sequence ObjectUID="s1"><Name>Simple Vlog 057</Name><VideoClipTrackItem ObjectUID="v1"><Media ObjectRef="m1"/></VideoClipTrackItem><VideoClipTrackItem ObjectUID="v2"><Media ObjectRef="m1"/></VideoClipTrackItem></Sequence><Sequence ObjectURef="s1"/><Sequence ObjectURef="s1"/>
      <Sequence ObjectUID="s2"><Name>pumpkin</Name></Sequence><Sequence ObjectURef="s2"/><Sequence ObjectURef="s2"/>
      <Sequence ObjectUID="s3"><Name>just the one skid for insta</Name></Sequence><Sequence ObjectURef="s3"/><Sequence ObjectURef="s3"/>
      <Sequence ObjectUID="s4"><Name>Simple Vlog 057_Sub_01</Name></Sequence><Sequence ObjectURef="s4"/><Sequence ObjectURef="s4"/>
    </Project></PremiereData>"#;

    #[test]
    fn resolves_definitions_references_bins_and_timeline_use() {
        let (media, sequences, bins, _) = build_index(parse_graph(FIXTURE).unwrap());
        assert_eq!(sequences.len(), 4);
        assert_eq!(sequences.iter().filter(|v| !v.nested).count(), 2);
        let main = sequences.iter().find(|v| v.id == "s1").unwrap();
        assert_eq!(main.timeline_placements, 2);
        assert_eq!(
            media
                .iter()
                .find(|v| v.path.ends_with("current.mp4"))
                .unwrap()
                .evidence,
            "used-in-saved-timeline"
        );
        assert_eq!(
            media
                .iter()
                .find(|v| v.path.ends_with("previous.mp4"))
                .unwrap()
                .evidence,
            "previously-referenced-in-saved-project"
        );
        assert!(bins.contains(&"Root".into()));
    }

    #[test]
    fn indexes_gzip_without_modifying_source() {
        let root = std::env::temp_dir().join(format!("pimp-prproj-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("Proof.prproj");
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(FIXTURE).unwrap();
        let bytes = encoder.finish().unwrap();
        fs::write(&path, &bytes).unwrap();
        let index = index_project(&path, "Proof.prproj", "now".into()).unwrap();
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(index.format_version.as_deref(), Some("3"));
        assert_eq!(index.sequences.len(), 4);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn omits_unnamed_definitions_and_rejects_unsafe_xml() {
        let (_, sequences, _, issues) = build_index(parse_graph(br#"<PremiereData><Sequence ObjectUID="s"/><Sequence ObjectURef="s"/></PremiereData>"#).unwrap());
        assert!(sequences.is_empty());
        assert!(issues.iter().any(|v| v.contains("omitted")));
        assert!(parse_graph(b"<!DOCTYPE x><PremiereData/>").is_err());
        assert!(parse_graph(b"<PremiereData><Sequence>").is_err());
        assert!(parse_graph(b"<PremiereData/><PremiereData/>").is_err());
        assert!(decoded_xml(b"not a project").is_err());
    }
}
