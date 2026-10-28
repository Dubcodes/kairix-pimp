use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};
use uuid::Uuid;

use crate::write_json;

pub const EVENT_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TechnicalMetadata {
    pub size_bytes: u64,
    pub duration_seconds: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub codec: Option<String>,
    pub frame_rate: Option<f64>,
    pub has_audio: Option<bool>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IntegrityEvidence {
    pub observed_size: u64,
    pub observed_modified_millis: Option<u64>,
    pub observed_sha256: Option<String>,
    pub needs_review: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MediaItem {
    pub id: String,
    pub relative_path: String,
    pub original_filename: String,
    pub sha256: String,
    pub capture_date: Option<String>,
    pub capture_date_provenance: String,
    pub ingest_date: String,
    #[serde(default)]
    pub file_type: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub project_ids: Vec<String>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub favourite: bool,
    #[serde(default)]
    pub technical: TechnicalMetadata,
    /// A cache-relative filename, never an absolute filesystem path.
    #[serde(default, alias = "thumbnailRef")]
    pub thumbnail_path: Option<String>,
    #[serde(default)]
    pub attached_audio_id: Option<String>,
    #[serde(default)]
    pub available: bool,
    #[serde(default)]
    pub needs_review: bool,
    #[serde(default)]
    pub observed_size: Option<u64>,
    #[serde(default)]
    pub observed_modified_millis: Option<u64>,
    #[serde(default)]
    pub integrity_by_device: BTreeMap<String, IntegrityEvidence>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRecord {
    pub id: String,
    pub name: String,
    pub working_directory: String,
    #[serde(default)]
    pub media_ids: Vec<String>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FolderRecord {
    pub id: String,
    pub relative_path: String,
    pub display_name: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MetadataConflict {
    pub entity_id: String,
    pub field: String,
    pub event_ids: Vec<String>,
    pub values: Vec<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CatalogueIssue {
    pub code: String,
    pub source: String,
    pub detail: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalogue {
    #[serde(default)]
    pub media: Vec<MediaItem>,
    #[serde(default)]
    pub projects: Vec<ProjectRecord>,
    #[serde(default)]
    pub folders: Vec<FolderRecord>,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub conflicts: Vec<MetadataConflict>,
    #[serde(default)]
    pub issues: Vec<CatalogueIssue>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum CatalogueEvent {
    #[serde(rename = "media.registered")]
    MediaRegistered { media: MediaItem },
    #[serde(rename = "media.tags.added")]
    MediaTagsAdded {
        #[serde(rename = "mediaId")]
        media_id: String,
        values: Vec<String>,
    },
    #[serde(rename = "media.tags.removed")]
    MediaTagsRemoved {
        #[serde(rename = "mediaId")]
        media_id: String,
        values: Vec<String>,
    },
    #[serde(rename = "media.tags.replaced")]
    MediaTagsReplaced {
        #[serde(rename = "mediaId")]
        media_id: String,
        values: Vec<String>,
    },
    #[serde(rename = "media.projects.added")]
    MediaProjectsAdded {
        #[serde(rename = "mediaId")]
        media_id: String,
        values: Vec<String>,
    },
    #[serde(rename = "media.projects.removed")]
    MediaProjectsRemoved {
        #[serde(rename = "mediaId")]
        media_id: String,
        values: Vec<String>,
    },
    #[serde(rename = "media.projects.replaced")]
    MediaProjectsReplaced {
        #[serde(rename = "mediaId")]
        media_id: String,
        values: Vec<String>,
    },
    #[serde(rename = "media.legacy-snapshot-replaced")]
    MediaLegacySnapshotReplaced { media: MediaItem },
    #[serde(rename = "media.notes.set")]
    MediaNotesSet {
        #[serde(rename = "mediaId")]
        media_id: String,
        value: String,
    },
    #[serde(rename = "media.favourite.set")]
    MediaFavouriteSet {
        #[serde(rename = "mediaId")]
        media_id: String,
        value: bool,
    },
    #[serde(rename = "media.enriched")]
    MediaEnriched {
        #[serde(rename = "mediaId")]
        media_id: String,
        technical: TechnicalMetadata,
        #[serde(rename = "thumbnailRef")]
        thumbnail_ref: Option<String>,
    },
    #[serde(rename = "media.evidence-observed")]
    MediaEvidenceObserved {
        #[serde(rename = "mediaId")]
        media_id: String,
        #[serde(rename = "observedSize")]
        observed_size: u64,
        #[serde(rename = "observedModifiedMillis")]
        observed_modified_millis: Option<u64>,
        #[serde(default, rename = "observedSha256")]
        observed_sha256: Option<String>,
        #[serde(rename = "needsReview")]
        needs_review: bool,
    },
    #[serde(rename = "folder.upserted")]
    FolderUpserted { folder: FolderRecord },
    #[serde(rename = "folder.moved")]
    FolderMoved {
        #[serde(rename = "folderId")]
        folder_id: String,
        #[serde(rename = "relativePath")]
        relative_path: String,
    },
    #[serde(rename = "project.upserted")]
    ProjectUpserted { project: ProjectRecord },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EventEnvelope {
    pub schema_version: u32,
    pub event_id: String,
    pub device_id: String,
    pub sequence: u64,
    pub occurred_at: String,
    #[serde(default)]
    pub observed: BTreeMap<String, u64>,
    pub event: CatalogueEvent,
}

#[derive(Clone, Debug, Default)]
pub struct EventRead {
    pub events: Vec<EventEnvelope>,
    pub vector: BTreeMap<String, u64>,
    pub issues: Vec<CatalogueIssue>,
}

fn normalized(values: &[String]) -> Vec<String> {
    let mut out: Vec<_> = values
        .iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect();
    out.sort_by(|a, b| {
        a.to_lowercase()
            .cmp(&b.to_lowercase())
            .then_with(|| a.cmp(b))
    });
    out.dedup_by(|a, b| a.to_lowercase() == b.to_lowercase());
    out
}

fn event_dir(root: &Path) -> PathBuf {
    root.join("events")
}

fn issue(code: &str, source: impl Into<String>, detail: impl Into<String>) -> CatalogueIssue {
    CatalogueIssue {
        code: code.into(),
        source: source.into(),
        detail: detail.into(),
    }
}

fn parse_legacy(value: &Value, line_number: u64) -> Result<EventEnvelope, String> {
    let device_id = value
        .get("deviceId")
        .and_then(Value::as_str)
        .unwrap_or("legacy")
        .to_string();
    let sequence = value
        .get("sequence")
        .and_then(Value::as_u64)
        .unwrap_or(line_number);
    let event_id = value
        .get("eventId")
        .and_then(Value::as_str)
        .map(String::from)
        .unwrap_or_else(|| format!("legacy-{device_id}-{sequence}"));
    let event_id = format!("v1-adapted-{event_id}");
    let occurred_at = value
        .get("occurredAt")
        .and_then(Value::as_str)
        .unwrap_or("1970-01-01T00:00:00Z")
        .to_string();
    let raw_event = value.get("event");
    let event_type = raw_event
        .and_then(|event| event.get("type"))
        .or_else(|| value.get("type"))
        .and_then(Value::as_str)
        .ok_or("legacy event type is missing")?;
    let body = raw_event.unwrap_or_else(|| value.get("payload").unwrap_or(value));
    let event = match event_type {
        "media.registered" | "media.discovered" => CatalogueEvent::MediaRegistered {
            media: serde_json::from_value(
                body.get("media").cloned().unwrap_or_else(|| body.clone()),
            )
            .map_err(|error| error.to_string())?,
        },
        "media.tags-set" => CatalogueEvent::MediaTagsReplaced {
            media_id: body["mediaId"].as_str().unwrap_or_default().into(),
            values: serde_json::from_value(body["tags"].clone()).unwrap_or_default(),
        },
        "media.projects-set" => CatalogueEvent::MediaProjectsReplaced {
            media_id: body["mediaId"].as_str().unwrap_or_default().into(),
            values: serde_json::from_value(body["projectIds"].clone()).unwrap_or_default(),
        },
        "project.upserted" => CatalogueEvent::ProjectUpserted {
            project: serde_json::from_value(
                body.get("project").cloned().unwrap_or_else(|| body.clone()),
            )
            .map_err(|error| error.to_string())?,
        },
        "folder.upserted" => CatalogueEvent::FolderUpserted {
            folder: serde_json::from_value(
                body.get("folder").cloned().unwrap_or_else(|| body.clone()),
            )
            .map_err(|error| error.to_string())?,
        },
        "folder.moved" => CatalogueEvent::FolderMoved {
            folder_id: body["folderId"].as_str().unwrap_or_default().into(),
            relative_path: body["relativePath"].as_str().unwrap_or_default().into(),
        },
        "media.updated" => CatalogueEvent::MediaLegacySnapshotReplaced {
            media: serde_json::from_value(body.clone()).map_err(|error| error.to_string())?,
        },
        other => return Err(format!("unsupported legacy event type {other}")),
    };
    let mut observed = BTreeMap::new();
    observed.insert(device_id.clone(), sequence.saturating_sub(1));
    Ok(EventEnvelope {
        schema_version: EVENT_SCHEMA_VERSION,
        event_id,
        device_id,
        sequence,
        occurred_at,
        observed,
        event,
    })
}

pub fn read_events(root: &Path) -> Result<EventRead, String> {
    fs::create_dir_all(event_dir(root)).map_err(|error| error.to_string())?;
    let mut result = EventRead::default();
    let mut entries: Vec<_> = fs::read_dir(event_dir(root))
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let filename = entry.file_name().to_string_lossy().to_string();
        let lower = filename.to_ascii_lowercase();
        if lower.contains(".sync-conflict-") {
            result.issues.push(issue(
                "syncthing-conflict-copy",
                filename,
                "Conflict copies are quarantined from authoritative replay",
            ));
            continue;
        }
        if path.extension().and_then(|value| value.to_str()) != Some("jsonl") {
            continue;
        }
        let bytes = fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        let content = String::from_utf8(bytes)
            .map_err(|error| format!("{} is not UTF-8: {error}", path.display()))?;
        if !content.is_empty() && !content.ends_with('\n') {
            result.issues.push(issue(
                "truncated-stream",
                filename.clone(),
                "Event stream does not end at a committed newline boundary",
            ));
        }
        let expected_device = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        let mut last_sequence = 0u64;
        let committed = if !content.is_empty() && !content.ends_with('\n') {
            content.rsplit_once('\n').map_or("", |(prefix, _)| prefix)
        } else {
            content.as_str()
        };
        for (index, line) in committed.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let location = format!("{filename}:{}", index + 1);
            let value: Value = match serde_json::from_str(line) {
                Ok(value) => value,
                Err(error) => {
                    result
                        .issues
                        .push(issue("malformed-event", location, error.to_string()));
                    continue;
                }
            };
            let version = value
                .get("schemaVersion")
                .and_then(Value::as_u64)
                .unwrap_or(1) as u32;
            if version > EVENT_SCHEMA_VERSION {
                result.issues.push(issue(
                    "unsupported-schema",
                    location,
                    format!("schema {version} is newer than {EVENT_SCHEMA_VERSION}"),
                ));
                continue;
            }
            let envelope = if version == EVENT_SCHEMA_VERSION {
                match serde_json::from_value::<EventEnvelope>(value) {
                    Ok(envelope)
                        if !envelope.event_id.is_empty()
                            && !envelope.device_id.is_empty()
                            && envelope.sequence > 0 =>
                    {
                        envelope
                    }
                    Ok(_) => {
                        result.issues.push(issue(
                            "invalid-envelope",
                            location,
                            "event ID, device ID and positive sequence are required",
                        ));
                        continue;
                    }
                    Err(error) => {
                        result
                            .issues
                            .push(issue("invalid-envelope", location, error.to_string()));
                        continue;
                    }
                }
            } else {
                match parse_legacy(&value, (index + 1) as u64) {
                    Ok(event) => event,
                    Err(error) => {
                        result.issues.push(issue("legacy-event", location, error));
                        continue;
                    }
                }
            };
            if version == EVENT_SCHEMA_VERSION && envelope.device_id != expected_device {
                result.issues.push(issue(
                    "stream-owner-mismatch",
                    location,
                    format!(
                        "event belongs to {} but is stored in {filename}",
                        envelope.device_id
                    ),
                ));
                continue;
            }
            if envelope.sequence <= last_sequence {
                result.issues.push(issue(
                    "non-monotonic-sequence",
                    location,
                    format!(
                        "sequence {} does not follow {}",
                        envelope.sequence, last_sequence
                    ),
                ));
                continue;
            }
            last_sequence = envelope.sequence;
            result
                .vector
                .entry(envelope.device_id.clone())
                .and_modify(|sequence| *sequence = (*sequence).max(envelope.sequence))
                .or_insert(envelope.sequence);
            result.events.push(envelope);
        }
    }
    Ok(result)
}

fn happens_before(a: &EventEnvelope, b: &EventEnvelope) -> bool {
    (a.device_id == b.device_id && a.sequence < b.sequence)
        || b.observed.get(&a.device_id).copied().unwrap_or(0) >= a.sequence
}

fn concurrent_key(event: &EventEnvelope) -> (u8, String, u64, String) {
    (
        match &event.event {
            CatalogueEvent::MediaRegistered { .. } if event.event_id.starts_with("legacy-") => 0,
            CatalogueEvent::MediaRegistered { .. } => 1,
            CatalogueEvent::MediaLegacySnapshotReplaced { .. } => 3,
            _ => 2,
        },
        event.device_id.clone(),
        event.sequence,
        event.event_id.clone(),
    )
}

fn causal_order(input: Vec<EventEnvelope>, issues: &mut Vec<CatalogueIssue>) -> Vec<EventEnvelope> {
    let mut unique = BTreeMap::<String, EventEnvelope>::new();
    for event in input {
        match unique.get(&event.event_id) {
            None => {
                unique.insert(event.event_id.clone(), event);
            }
            Some(previous) if previous != &event => {
                issues.push(issue(
                    "duplicate-event-conflict",
                    event.event_id.clone(),
                    "Duplicate event ID has different content; deterministic copy retained",
                ));
                let incoming = serde_json::to_string(&event).unwrap();
                let existing = serde_json::to_string(previous).unwrap();
                if incoming < existing {
                    unique.insert(event.event_id.clone(), event);
                }
            }
            Some(_) => {}
        }
    }
    let mut nodes: Vec<_> = unique.into_values().collect();
    nodes.sort_by_key(concurrent_key);
    let mut by_device = BTreeMap::<String, Vec<usize>>::new();
    for (index, event) in nodes.iter().enumerate() {
        by_device
            .entry(event.device_id.clone())
            .or_default()
            .push(index);
    }
    for indexes in by_device.values_mut() {
        indexes.sort_by_key(|index| nodes[*index].sequence);
    }
    let mut outgoing = vec![BTreeSet::<usize>::new(); nodes.len()];
    let mut indegree = vec![0usize; nodes.len()];
    let add_edge =
        |from: usize, to: usize, outgoing: &mut Vec<BTreeSet<usize>>, indegree: &mut Vec<usize>| {
            if from != to && outgoing[from].insert(to) {
                indegree[to] += 1;
            }
        };
    for (device_id, indexes) in &by_device {
        for pair in indexes.windows(2) {
            let previous = &nodes[pair[0]];
            let current = &nodes[pair[1]];
            add_edge(pair[0], pair[1], &mut outgoing, &mut indegree);
            if current.sequence != previous.sequence + 1 {
                issues.push(issue(
                    "missing-causal-dependency",
                    current.event_id.clone(),
                    format!(
                        "{device_id} sequence {} follows {}",
                        current.sequence, previous.sequence
                    ),
                ));
            }
        }
    }
    for (index, event) in nodes.iter().enumerate() {
        for (device_id, observed_sequence) in &event.observed {
            if device_id == &event.device_id && *observed_sequence >= event.sequence {
                issues.push(issue(
                    "invalid-causal-metadata",
                    event.event_id.clone(),
                    "An event cannot observe its own sequence or a future one",
                ));
                continue;
            }
            if *observed_sequence == 0 {
                continue;
            }
            let dependency = by_device.get(device_id).and_then(|indexes| {
                indexes
                    .iter()
                    .copied()
                    .filter(|candidate| nodes[*candidate].sequence <= *observed_sequence)
                    .next_back()
            });
            if dependency
                .map(|candidate| nodes[candidate].sequence < *observed_sequence)
                .unwrap_or(true)
            {
                issues.push(issue(
                    "missing-causal-dependency",
                    event.event_id.clone(),
                    format!(
                        "Observed {device_id} sequence {observed_sequence}, which is not available"
                    ),
                ));
            }
            if let Some(dependency) = dependency {
                add_edge(dependency, index, &mut outgoing, &mut indegree);
            }
        }
    }
    let mut ready = BTreeSet::<((u8, String, u64, String), usize)>::new();
    for (index, degree) in indegree.iter().enumerate() {
        if *degree == 0 {
            ready.insert((concurrent_key(&nodes[index]), index));
        }
    }
    let mut emitted = vec![false; nodes.len()];
    let mut ordered = Vec::with_capacity(nodes.len());
    while let Some((key, index)) = ready.pop_first() {
        let _ = key;
        if emitted[index] {
            continue;
        }
        emitted[index] = true;
        ordered.push(nodes[index].clone());
        for dependent in &outgoing[index] {
            indegree[*dependent] -= 1;
            if indegree[*dependent] == 0 {
                ready.insert((concurrent_key(&nodes[*dependent]), *dependent));
            }
        }
    }
    if ordered.len() != nodes.len() {
        issues.push(issue(
            "causal-cycle",
            "event-streams",
            "Cyclic causal metadata was detected; affected events use deterministic fallback order",
        ));
        let mut remaining: Vec<_> = nodes
            .into_iter()
            .enumerate()
            .filter(|(index, _)| !emitted[*index])
            .map(|(_, event)| event)
            .collect();
        remaining.sort_by_key(concurrent_key);
        ordered.extend(remaining);
    }
    ordered
}

#[derive(Clone)]
struct Addition {
    value: String,
    envelope: EventEnvelope,
}

fn add_conflict(catalogue: &mut Catalogue, conflict: MetadataConflict) {
    if !catalogue.conflicts.iter().any(|candidate| {
        candidate.entity_id == conflict.entity_id
            && candidate.field == conflict.field
            && candidate.event_ids == conflict.event_ids
    }) {
        catalogue.conflicts.push(conflict);
    }
}

pub fn rebuild(mut read: EventRead) -> Catalogue {
    let events = causal_order(std::mem::take(&mut read.events), &mut read.issues);
    let mut catalogue = Catalogue::default();
    let mut seen = HashSet::new();
    let mut media = HashMap::<String, MediaItem>::new();
    let mut projects = HashMap::<String, ProjectRecord>::new();
    let mut folders = HashMap::<String, FolderRecord>::new();
    let mut additions = HashMap::<String, HashMap<String, Addition>>::new();
    let mut scalar_versions = HashMap::<String, (EventEnvelope, Value)>::new();

    for envelope in events {
        if !seen.insert(envelope.event_id.clone()) {
            continue;
        }
        match &envelope.event {
            CatalogueEvent::MediaRegistered { media: item } => {
                if let Some(existing) = media.get(&item.id) {
                    if existing != item {
                        add_conflict(
                            &mut catalogue,
                            MetadataConflict {
                                entity_id: item.id.clone(),
                                field: "registration".into(),
                                event_ids: vec![envelope.event_id.clone()],
                                values: vec![
                                    serde_json::to_value(existing).unwrap(),
                                    serde_json::to_value(item).unwrap(),
                                ],
                            },
                        );
                    }
                    continue;
                }
                let mut item = item.clone();
                if item
                    .thumbnail_path
                    .as_deref()
                    .is_some_and(|value| Path::new(value).is_absolute())
                {
                    item.thumbnail_path = Some(format!("{}.jpg", item.id));
                }
                let original_tags = std::mem::take(&mut item.tags);
                let original_projects = std::mem::take(&mut item.project_ids);
                for (field, values) in [("tags", original_tags), ("projectIds", original_projects)]
                {
                    let set = additions.entry(format!("{}:{field}", item.id)).or_default();
                    for value in normalized(&values) {
                        set.insert(
                            format!("{}:{value}", envelope.event_id),
                            Addition {
                                value,
                                envelope: envelope.clone(),
                            },
                        );
                    }
                }
                media.insert(item.id.clone(), item);
            }
            CatalogueEvent::MediaLegacySnapshotReplaced { media: item } => {
                if let Some(existing) = media.get(&item.id) {
                    if existing != item {
                        add_conflict(
                            &mut catalogue,
                            MetadataConflict {
                                entity_id: item.id.clone(),
                                field: "legacy-snapshot".into(),
                                event_ids: vec![envelope.event_id.clone()],
                                values: vec![
                                    serde_json::to_value(existing).unwrap(),
                                    serde_json::to_value(item).unwrap(),
                                ],
                            },
                        );
                    }
                }
                let mut item = item.clone();
                let original_tags = std::mem::take(&mut item.tags);
                let original_projects = std::mem::take(&mut item.project_ids);
                for (field, values) in [("tags", original_tags), ("projectIds", original_projects)]
                {
                    let mut set = HashMap::new();
                    for value in normalized(&values) {
                        set.insert(
                            format!("{}:{value}", envelope.event_id),
                            Addition {
                                value,
                                envelope: envelope.clone(),
                            },
                        );
                    }
                    additions.insert(format!("{}:{field}", item.id), set);
                }
                media.insert(item.id.clone(), item);
            }
            CatalogueEvent::MediaTagsAdded { media_id, values }
            | CatalogueEvent::MediaProjectsAdded { media_id, values } => {
                if !media.contains_key(media_id) {
                    add_conflict(
                        &mut catalogue,
                        MetadataConflict {
                            entity_id: media_id.clone(),
                            field: "missing-media".into(),
                            event_ids: vec![envelope.event_id.clone()],
                            values: vec![serde_json::to_value(&envelope.event).unwrap()],
                        },
                    );
                    continue;
                }
                let field = if matches!(envelope.event, CatalogueEvent::MediaTagsAdded { .. }) {
                    "tags"
                } else {
                    "projectIds"
                };
                let set = additions.entry(format!("{media_id}:{field}")).or_default();
                for value in normalized(values) {
                    set.insert(
                        format!("{}:{value}", envelope.event_id),
                        Addition {
                            value,
                            envelope: envelope.clone(),
                        },
                    );
                }
            }
            CatalogueEvent::MediaTagsRemoved { media_id, values }
            | CatalogueEvent::MediaProjectsRemoved { media_id, values } => {
                let field = if matches!(envelope.event, CatalogueEvent::MediaTagsRemoved { .. }) {
                    "tags"
                } else {
                    "projectIds"
                };
                let removed: HashSet<_> = normalized(values)
                    .into_iter()
                    .map(|value| value.to_lowercase())
                    .collect();
                if let Some(set) = additions.get_mut(&format!("{media_id}:{field}")) {
                    set.retain(|_, addition| {
                        !removed.contains(&addition.value.to_lowercase())
                            || !happens_before(&addition.envelope, &envelope)
                    });
                }
            }
            CatalogueEvent::MediaTagsReplaced { media_id, values }
            | CatalogueEvent::MediaProjectsReplaced { media_id, values } => {
                if !media.contains_key(media_id) {
                    add_conflict(
                        &mut catalogue,
                        MetadataConflict {
                            entity_id: media_id.clone(),
                            field: "missing-media".into(),
                            event_ids: vec![envelope.event_id.clone()],
                            values: vec![serde_json::to_value(&envelope.event).unwrap()],
                        },
                    );
                    continue;
                }
                let field = if matches!(envelope.event, CatalogueEvent::MediaTagsReplaced { .. }) {
                    "tags"
                } else {
                    "projectIds"
                };
                let set = additions.entry(format!("{media_id}:{field}")).or_default();
                set.retain(|_, addition| !happens_before(&addition.envelope, &envelope));
                for value in normalized(values) {
                    set.insert(
                        format!("{}:{value}", envelope.event_id),
                        Addition {
                            value,
                            envelope: envelope.clone(),
                        },
                    );
                }
            }
            CatalogueEvent::MediaNotesSet { media_id, value } => {
                let Some(item) = media.get_mut(media_id) else {
                    continue;
                };
                let key = format!("{media_id}:notes");
                if let Some((previous, previous_value)) = scalar_versions.get(&key) {
                    if !happens_before(previous, &envelope) {
                        add_conflict(
                            &mut catalogue,
                            MetadataConflict {
                                entity_id: media_id.clone(),
                                field: "notes".into(),
                                event_ids: vec![
                                    previous.event_id.clone(),
                                    envelope.event_id.clone(),
                                ],
                                values: vec![previous_value.clone(), Value::String(value.clone())],
                            },
                        );
                        let prior_key = format!(
                            "{}:{:020}:{}",
                            previous.device_id, previous.sequence, previous.event_id
                        );
                        let next_key = format!(
                            "{}:{:020}:{}",
                            envelope.device_id, envelope.sequence, envelope.event_id
                        );
                        if next_key > prior_key {
                            item.notes = value.clone();
                        }
                    } else {
                        item.notes = value.clone();
                    }
                } else {
                    item.notes = value.clone();
                }
                scalar_versions.insert(key, (envelope.clone(), Value::String(value.clone())));
            }
            CatalogueEvent::MediaFavouriteSet { media_id, value } => {
                let Some(item) = media.get_mut(media_id) else {
                    continue;
                };
                let key = format!("{media_id}:favourite");
                if let Some((previous, previous_value)) = scalar_versions.get(&key) {
                    if !happens_before(previous, &envelope) {
                        add_conflict(
                            &mut catalogue,
                            MetadataConflict {
                                entity_id: media_id.clone(),
                                field: "favourite".into(),
                                event_ids: vec![
                                    previous.event_id.clone(),
                                    envelope.event_id.clone(),
                                ],
                                values: vec![previous_value.clone(), Value::Bool(value.clone())],
                            },
                        );
                        let prior_key = format!(
                            "{}:{:020}:{}",
                            previous.device_id, previous.sequence, previous.event_id
                        );
                        let next_key = format!(
                            "{}:{:020}:{}",
                            envelope.device_id, envelope.sequence, envelope.event_id
                        );
                        if next_key > prior_key {
                            item.favourite = *value;
                        }
                    } else {
                        item.favourite = *value;
                    }
                } else {
                    item.favourite = *value;
                }
                scalar_versions.insert(key, (envelope.clone(), Value::Bool(*value)));
            }
            CatalogueEvent::MediaEnriched {
                media_id,
                technical,
                thumbnail_ref,
            } => {
                if let Some(item) = media.get_mut(media_id) {
                    item.technical = technical.clone();
                    item.thumbnail_path = thumbnail_ref
                        .as_ref()
                        .filter(|value| safe_cache_reference(value))
                        .cloned();
                }
            }
            CatalogueEvent::MediaEvidenceObserved {
                media_id,
                observed_size,
                observed_modified_millis,
                observed_sha256,
                needs_review,
            } => {
                if let Some(item) = media.get_mut(media_id) {
                    item.observed_size = Some(*observed_size);
                    item.observed_modified_millis = *observed_modified_millis;
                    item.needs_review = *needs_review;
                    item.integrity_by_device.insert(
                        envelope.device_id.clone(),
                        IntegrityEvidence {
                            observed_size: *observed_size,
                            observed_modified_millis: *observed_modified_millis,
                            observed_sha256: observed_sha256.clone(),
                            needs_review: *needs_review,
                        },
                    );
                }
            }
            CatalogueEvent::FolderUpserted { folder } => {
                let key = format!("{}:folder", folder.id);
                let value = serde_json::to_value(folder).unwrap();
                if let Some((previous, previous_value)) = scalar_versions.get(&key) {
                    if !happens_before(previous, &envelope) && previous_value != &value {
                        add_conflict(
                            &mut catalogue,
                            MetadataConflict {
                                entity_id: folder.id.clone(),
                                field: "folder".into(),
                                event_ids: vec![
                                    previous.event_id.clone(),
                                    envelope.event_id.clone(),
                                ],
                                values: vec![previous_value.clone(), value.clone()],
                            },
                        );
                    }
                }
                let mut folder = folder.clone();
                folder.tags = normalized(&folder.tags);
                folders.insert(folder.id.clone(), folder);
                scalar_versions.insert(key, (envelope.clone(), value));
            }
            CatalogueEvent::FolderMoved {
                folder_id,
                relative_path,
            } => {
                let Some(folder) = folders.get_mut(folder_id) else {
                    add_conflict(
                        &mut catalogue,
                        MetadataConflict {
                            entity_id: folder_id.clone(),
                            field: "missing-folder".into(),
                            event_ids: vec![envelope.event_id.clone()],
                            values: vec![Value::String(relative_path.clone())],
                        },
                    );
                    continue;
                };
                let key = format!("{folder_id}:folder-path");
                if let Some((previous, previous_value)) = scalar_versions.get(&key) {
                    if !happens_before(previous, &envelope) {
                        add_conflict(
                            &mut catalogue,
                            MetadataConflict {
                                entity_id: folder_id.clone(),
                                field: "relativePath".into(),
                                event_ids: vec![
                                    previous.event_id.clone(),
                                    envelope.event_id.clone(),
                                ],
                                values: vec![
                                    previous_value.clone(),
                                    Value::String(relative_path.clone()),
                                ],
                            },
                        );
                    }
                }
                folder.relative_path = relative_path.clone();
                scalar_versions.insert(
                    key,
                    (envelope.clone(), Value::String(relative_path.clone())),
                );
            }
            CatalogueEvent::ProjectUpserted { project } => {
                let key = format!("{}:project", project.id);
                if let Some((previous, previous_value)) = scalar_versions.get(&key) {
                    if !happens_before(previous, &envelope)
                        && previous_value != &serde_json::to_value(project).unwrap()
                    {
                        add_conflict(
                            &mut catalogue,
                            MetadataConflict {
                                entity_id: project.id.clone(),
                                field: "project".into(),
                                event_ids: vec![
                                    previous.event_id.clone(),
                                    envelope.event_id.clone(),
                                ],
                                values: vec![
                                    previous_value.clone(),
                                    serde_json::to_value(project).unwrap(),
                                ],
                            },
                        );
                    }
                }
                projects.insert(project.id.clone(), project.clone());
                scalar_versions.insert(
                    key,
                    (envelope.clone(), serde_json::to_value(project).unwrap()),
                );
            }
        }
    }

    for item in media.values_mut() {
        item.tags = normalized(
            &additions
                .get(&format!("{}:tags", item.id))
                .map(|set| {
                    set.values()
                        .map(|addition| addition.value.clone())
                        .collect::<Vec<String>>()
                })
                .unwrap_or_default(),
        );
        item.project_ids = normalized(
            &additions
                .get(&format!("{}:projectIds", item.id))
                .map(|set| {
                    set.values()
                        .map(|addition| addition.value.clone())
                        .collect::<Vec<String>>()
                })
                .unwrap_or_default(),
        );
    }
    for project in projects.values_mut() {
        project.media_ids = media
            .values()
            .filter(|item| item.project_ids.contains(&project.id))
            .map(|item| item.id.clone())
            .collect();
        project.media_ids.sort();
    }
    catalogue.media = media.into_values().collect();
    catalogue
        .media
        .sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    catalogue.projects = projects.into_values().collect();
    catalogue.projects.sort_by(|a, b| a.name.cmp(&b.name));
    catalogue.folders = folders.into_values().collect();
    catalogue
        .folders
        .sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    catalogue.issues.extend(read.issues);
    catalogue.updated_at = Utc::now().to_rfc3339();
    pair_audio(&mut catalogue);
    catalogue
}

pub fn load(root: &Path) -> Result<Catalogue, String> {
    Ok(rebuild(read_events(root)?))
}

pub fn event_signature(root: &Path) -> Result<String, String> {
    let directory = event_dir(root);
    if !directory.is_dir() {
        return Ok(sha256_bytes(b"empty-event-directory"));
    }
    let mut entries = fs::read_dir(&directory)
        .map_err(|error| error.to_string())?
        .map(|entry| {
            let entry = entry.map_err(|error| error.to_string())?;
            if entry.path().extension().and_then(|value| value.to_str()) != Some("jsonl") {
                return Ok(None);
            }
            let metadata = entry.metadata().map_err(|error| error.to_string())?;
            let modified = metadata
                .modified()
                .ok()
                .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|value| value.as_nanos())
                .unwrap_or_default();
            Ok(Some(format!(
                "{}\0{}\0{}",
                entry.file_name().to_string_lossy(),
                metadata.len(),
                modified
            )))
        })
        .collect::<Result<Vec<_>, String>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    entries.sort();
    Ok(sha256_bytes(entries.join("\n").as_bytes()))
}

pub fn append(root: &Path, device_id: &str, events: Vec<CatalogueEvent>) -> Result<(), String> {
    if device_id.is_empty()
        || Path::new(device_id)
            .file_name()
            .and_then(|value| value.to_str())
            != Some(device_id)
        || device_id.to_ascii_lowercase().contains(".sync-conflict-")
    {
        return Err("Device identity is unsafe for an owned event stream".into());
    }
    if events.is_empty() {
        return Ok(());
    }
    let read = read_events(root)?;
    let mut vector = read.vector;
    let mut sequence = vector.get(device_id).copied().unwrap_or(0);
    let directory = event_dir(root);
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let path = directory.join(format!("{device_id}.jsonl"));
    if path.exists() {
        let bytes = fs::read(&path).map_err(|error| error.to_string())?;
        if !bytes.is_empty() && bytes.last() != Some(&b'\n') {
            return Err(format!(
                "Refusing to append to truncated event stream {}",
                path.display()
            ));
        }
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| error.to_string())?;
    for event in events {
        sequence += 1;
        vector.insert(device_id.to_string(), sequence - 1);
        let envelope = EventEnvelope {
            schema_version: EVENT_SCHEMA_VERSION,
            event_id: Uuid::new_v4().to_string(),
            device_id: device_id.into(),
            sequence,
            occurred_at: Utc::now().to_rfc3339(),
            observed: vector.clone(),
            event,
        };
        writeln!(file, "{}", serde_json::to_string(&envelope).unwrap())
            .map_err(|error| error.to_string())?;
        vector.insert(device_id.to_string(), sequence);
    }
    file.sync_all().map_err(|error| error.to_string())
}

pub fn safe_cache_reference(value: &str) -> bool {
    !value.is_empty()
        && !Path::new(value).is_absolute()
        && !value.contains('/')
        && !value.contains('\\')
        && !value.contains("..")
        && value.ends_with(".jpg")
}

pub fn pair_audio(catalogue: &mut Catalogue) {
    let mut audios = HashMap::<(String, String), Vec<String>>::new();
    for item in catalogue
        .media
        .iter()
        .filter(|item| item.file_type == "wav")
    {
        let path = Path::new(&item.relative_path);
        let directory = path
            .parent()
            .unwrap_or(Path::new(""))
            .to_string_lossy()
            .replace('\\', "/")
            .to_ascii_lowercase();
        let stem = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_ascii_lowercase();
        audios
            .entry((directory, stem))
            .or_default()
            .push(item.id.clone());
    }
    for item in catalogue
        .media
        .iter_mut()
        .filter(|item| matches!(item.file_type.as_str(), "mp4" | "mov" | "m4v"))
    {
        let path = Path::new(&item.relative_path);
        let key = (
            path.parent()
                .unwrap_or(Path::new(""))
                .to_string_lossy()
                .replace('\\', "/")
                .to_ascii_lowercase(),
            path.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_ascii_lowercase(),
        );
        item.attached_audio_id = audios
            .get(&key)
            .filter(|matches| matches.len() == 1)
            .map(|matches| matches[0].clone());
    }
}

pub fn migrate_legacy_snapshot(root: &Path, _device_id: &str) -> Result<(), String> {
    fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let marker = root.join("migration-v2.json");
    if marker.exists() {
        let value: Value = serde_json::from_slice(&fs::read(&marker).map_err(|e| e.to_string())?)
            .map_err(|error| format!("Invalid migration marker: {error}"))?;
        if value.get("source").and_then(Value::as_str) == Some("none") {
            if !root.join("catalogue.json").exists() {
                return Ok(());
            }
        } else if value.get("source").and_then(Value::as_str) == Some("catalogue.json") {
            let backup = value
                .get("backup")
                .and_then(Value::as_str)
                .unwrap_or("catalogue.pre-events-v1.backup.json");
            if !root.join(backup).is_file() {
                return Err("Migration marker exists but its legacy backup is missing".into());
            }
            let stream = value
                .get("eventStream")
                .and_then(Value::as_str)
                .ok_or("Migration marker does not identify its event stream")?;
            let backup_bytes = fs::read(root.join(backup)).map_err(|error| error.to_string())?;
            let expected_hash = value
                .get("sourceSha256")
                .and_then(Value::as_str)
                .ok_or("Migration marker does not identify the migrated source hash")?;
            if sha256_bytes(&backup_bytes) != expected_hash {
                return Err("Migration backup does not match the completed source hash".into());
            }
            let legacy: Catalogue = serde_json::from_slice(&backup_bytes)
                .map_err(|error| format!("Migration backup is invalid: {error}"))?;
            let bytes = fs::read(event_dir(root).join(stream)).map_err(|_| {
                "Migration marker exists but its event stream is missing".to_string()
            })?;
            if !bytes.is_empty() && !bytes.ends_with(b"\n") {
                return Err("Migration event stream is truncated".into());
            }
            if bytes != migration_content(&legacy).as_bytes() {
                return Err("Migration event stream does not match its source snapshot".into());
            }
            return Ok(());
        }
    }
    let legacy_path = root.join("catalogue.json");
    if !legacy_path.exists() {
        return Ok(());
    }
    let bytes = fs::read(&legacy_path).map_err(|error| error.to_string())?;
    let legacy_value: Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Legacy catalogue migration stopped: {error}"))?;
    let recognizable = legacy_value.get("media").is_some()
        || legacy_value.get("projects").is_some()
        || legacy_value.get("format").and_then(Value::as_str) == Some("pimp-event-log");
    if !recognizable {
        return Err("Legacy catalogue migration stopped: unrecognized catalogue structure".into());
    }
    let legacy: Catalogue = serde_json::from_value(legacy_value)
        .map_err(|error| format!("Legacy catalogue migration stopped: {error}"))?;
    let backup = root.join("catalogue.pre-events-v1.backup.json");
    if !backup.exists() {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&backup)
            .map_err(|error| error.to_string())?;
        file.write_all(&bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
    } else if fs::read(&backup).map_err(|error| error.to_string())? != bytes {
        return Err(
            "Existing migration backup differs from catalogue.json; refusing to continue".into(),
        );
    }
    let source_sha256 = sha256_bytes(&bytes);
    let migration_path = event_dir(root).join("migration-v2.jsonl");
    if !migration_path.exists() {
        fs::create_dir_all(event_dir(root)).map_err(|error| error.to_string())?;
        let content = migration_content(&legacy);
        let temporary = root.join(format!(".migration-{}.tmp", Uuid::new_v4()));
        fs::write(&temporary, &content).map_err(|error| error.to_string())?;
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(&temporary)
            .and_then(|file| file.sync_all())
            .map_err(|error| error.to_string())?;
        if let Err(error) = fs::rename(&temporary, &migration_path) {
            if !migration_path.is_file()
                || fs::read(&migration_path).map_err(|read_error| read_error.to_string())?
                    != content.as_bytes()
            {
                return Err(error.to_string());
            }
            let _ = fs::remove_file(&temporary);
        }
    } else if fs::read(&migration_path).map_err(|error| error.to_string())?
        != migration_content(&legacy).as_bytes()
    {
        return Err("Existing migration stream differs from catalogue.json".into());
    }
    write_json(
        &marker,
        &serde_json::json!({"schemaVersion":2,"migratedAt":Utc::now().to_rfc3339(),"source":"catalogue.json","sourceSha256":source_sha256,"backup":"catalogue.pre-events-v1.backup.json","eventStream":migration_path.file_name().unwrap().to_string_lossy(),"migrationOwner":"deterministic-v2"}),
    )
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn migration_content(legacy: &Catalogue) -> String {
    let migration_device = "migration-v2".to_string();
    let mut content = String::new();
    let mut sequence = 0u64;
    let mut push = |event: CatalogueEvent, identity: String| {
        sequence += 1;
        let observed = BTreeMap::from([(migration_device.clone(), sequence - 1)]);
        let envelope = EventEnvelope {
            schema_version: EVENT_SCHEMA_VERSION,
            event_id: format!("legacy-{identity}"),
            device_id: migration_device.clone(),
            sequence,
            occurred_at: "1970-01-01T00:00:00Z".into(),
            observed,
            event,
        };
        content.push_str(&serde_json::to_string(&envelope).unwrap());
        content.push('\n');
    };
    for media in &legacy.media {
        push(
            CatalogueEvent::MediaLegacySnapshotReplaced {
                media: media.clone(),
            },
            format!("media-{}", media.id),
        );
    }
    for project in &legacy.projects {
        push(
            CatalogueEvent::ProjectUpserted {
                project: project.clone(),
            },
            format!("project-{}", project.id),
        );
    }
    for folder in &legacy.folders {
        push(
            CatalogueEvent::FolderUpserted {
                folder: folder.clone(),
            },
            format!("folder-{}", folder.id),
        );
    }
    content
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary(name: &str) -> PathBuf {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(".test-tmp")
            .join(format!("pimp-catalogue-{name}-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn item(id: &str, path: &str, kind: &str) -> MediaItem {
        MediaItem {
            id: id.into(),
            relative_path: path.into(),
            original_filename: Path::new(path)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into(),
            sha256: format!("hash-{id}"),
            capture_date: None,
            capture_date_provenance: "unknown".into(),
            ingest_date: "2026-10-22T00:00:00Z".into(),
            file_type: kind.into(),
            tags: vec![],
            project_ids: vec![],
            notes: String::new(),
            favourite: false,
            technical: TechnicalMetadata {
                size_bytes: 1,
                ..Default::default()
            },
            thumbnail_path: None,
            attached_audio_id: None,
            available: true,
            needs_review: false,
            observed_size: Some(1),
            observed_modified_millis: Some(1),
            integrity_by_device: Default::default(),
        }
    }

    fn envelope(device: &str, sequence: u64, event: CatalogueEvent) -> EventEnvelope {
        EventEnvelope {
            schema_version: EVENT_SCHEMA_VERSION,
            event_id: format!("{device}-{sequence}"),
            device_id: device.into(),
            sequence,
            occurred_at: "2026-10-22T00:00:00Z".into(),
            observed: BTreeMap::from([(device.into(), sequence.saturating_sub(1))]),
            event,
        }
    }

    #[test]
    fn independently_added_tags_converge_in_any_delivery_order() {
        let registered = envelope(
            "seed",
            1,
            CatalogueEvent::MediaRegistered {
                media: item("m", "2026-10-22/a.mp4", "mp4"),
            },
        );
        let desktop = envelope(
            "desktop",
            1,
            CatalogueEvent::MediaTagsAdded {
                media_id: "m".into(),
                values: vec!["Track".into()],
            },
        );
        let laptop = envelope(
            "laptop",
            1,
            CatalogueEvent::MediaTagsAdded {
                media_id: "m".into(),
                values: vec!["Night".into()],
            },
        );
        let first = rebuild(EventRead {
            events: vec![desktop.clone(), registered.clone(), laptop.clone()],
            ..Default::default()
        });
        let second = rebuild(EventRead {
            events: vec![laptop, desktop, registered],
            ..Default::default()
        });
        assert_eq!(first.media[0].tags, vec!["Night", "Track"]);
        assert_eq!(first.media[0].tags, second.media[0].tags);
    }

    #[test]
    fn metadata_normalization_is_case_insensitive_unicode_aware_and_convergent() {
        let registered = envelope(
            "normalizer",
            1,
            CatalogueEvent::MediaRegistered {
                media: item("m", "2026-10-22/a.mp4", "mp4"),
            },
        );
        let variants = envelope(
            "normalizer",
            2,
            CatalogueEvent::MediaTagsAdded {
                media_id: "m".into(),
                values: vec![" Race ".into(), "race".into(), "Ångström".into()],
            },
        );
        let more_variants = envelope(
            "normalizer",
            3,
            CatalogueEvent::MediaTagsAdded {
                media_id: "m".into(),
                values: vec!["RACE".into(), " ångström ".into(), "Race".into()],
            },
        );
        let first = rebuild(EventRead {
            events: vec![
                registered.clone(),
                variants.clone(),
                more_variants.clone(),
                more_variants.clone(),
            ],
            ..Default::default()
        });
        let second = rebuild(EventRead {
            events: vec![more_variants, variants, registered],
            ..Default::default()
        });
        assert_eq!(first.media[0].tags, vec!["RACE", "Ångström"]);
        assert_eq!(first.media[0].tags, second.media[0].tags);
    }

    #[test]
    fn metadata_removal_matches_the_same_case_insensitive_key() {
        let events = vec![
            envelope(
                "normalizer",
                1,
                CatalogueEvent::MediaRegistered {
                    media: item("m", "2026-10-22/a.mp4", "mp4"),
                },
            ),
            envelope(
                "normalizer",
                2,
                CatalogueEvent::MediaTagsAdded {
                    media_id: "m".into(),
                    values: vec!["Race".into(), "Ångström".into()],
                },
            ),
            envelope(
                "normalizer",
                3,
                CatalogueEvent::MediaTagsRemoved {
                    media_id: "m".into(),
                    values: vec!["race".into()],
                },
            ),
        ];
        assert_eq!(
            rebuild(EventRead {
                events,
                ..Default::default()
            })
            .media[0]
                .tags,
            vec!["Ångström"]
        );
    }

    #[test]
    fn three_device_causal_replay_converges_for_every_delivery_order() {
        let z = envelope(
            "z-device",
            1,
            CatalogueEvent::MediaRegistered {
                media: item("m", "2026-10-22/a.mp4", "mp4"),
            },
        );
        let mut a = envelope(
            "a-device",
            1,
            CatalogueEvent::MediaTagsAdded {
                media_id: "m".into(),
                values: vec!["Observed Z".into()],
            },
        );
        a.observed.insert("z-device".into(), 1);
        let b = envelope(
            "b-device",
            1,
            CatalogueEvent::MediaTagsAdded {
                media_id: "m".into(),
                values: vec!["Concurrent B".into()],
            },
        );
        let permutations = vec![
            vec![z.clone(), a.clone(), b.clone()],
            vec![z.clone(), b.clone(), a.clone()],
            vec![a.clone(), z.clone(), b.clone()],
            vec![a.clone(), b.clone(), z.clone()],
            vec![b.clone(), z.clone(), a.clone()],
            vec![b, a, z],
        ];
        let projections: Vec<_> = permutations
            .into_iter()
            .map(|events| {
                rebuild(EventRead {
                    events,
                    ..Default::default()
                })
            })
            .collect();
        for projection in &projections {
            assert_eq!(projection.media[0].tags, vec!["Concurrent B", "Observed Z"]);
            assert!(projection.issues.is_empty());
        }
        for projection in &projections[1..] {
            assert_eq!(projection.media, projections[0].media);
            assert_eq!(projection.conflicts, projections[0].conflicts);
        }
    }

    #[test]
    fn missing_causal_dependencies_are_reported_without_dropping_valid_events() {
        let mut dependent = envelope(
            "a-device",
            1,
            CatalogueEvent::MediaRegistered {
                media: item("m", "a.mp4", "mp4"),
            },
        );
        dependent.observed.insert("z-device".into(), 3);
        let catalogue = rebuild(EventRead {
            events: vec![dependent],
            ..Default::default()
        });
        assert_eq!(catalogue.media.len(), 1);
        assert!(catalogue
            .issues
            .iter()
            .any(|issue| issue.code == "missing-causal-dependency"));
    }

    #[test]
    fn disk_level_device_stream_merge_converges_for_media_tags_and_projects() {
        let desktop_root = temporary("desktop-stream");
        let laptop_root = temporary("laptop-stream");
        let first_merge = temporary("merge-first");
        let second_merge = temporary("merge-second");
        let seed = envelope(
            "seed",
            1,
            CatalogueEvent::MediaRegistered {
                media: item("m", "2026-10-22/a.mp4", "mp4"),
            },
        );
        for root in [&desktop_root, &laptop_root] {
            fs::create_dir_all(event_dir(root)).unwrap();
            fs::write(
                event_dir(root).join("seed.jsonl"),
                format!("{}\n", serde_json::to_string(&seed).unwrap()),
            )
            .unwrap();
        }
        append(
            &desktop_root,
            "desktop",
            vec![
                CatalogueEvent::MediaTagsAdded {
                    media_id: "m".into(),
                    values: vec!["Track".into()],
                },
                CatalogueEvent::MediaProjectsAdded {
                    media_id: "m".into(),
                    values: vec!["project-a".into()],
                },
            ],
        )
        .unwrap();
        append(
            &laptop_root,
            "laptop",
            vec![
                CatalogueEvent::MediaRegistered {
                    media: item("m2", "2026-10-23/b.mp4", "mp4"),
                },
                CatalogueEvent::MediaTagsAdded {
                    media_id: "m".into(),
                    values: vec!["Night".into()],
                },
                CatalogueEvent::MediaProjectsAdded {
                    media_id: "m".into(),
                    values: vec!["project-b".into()],
                },
            ],
        )
        .unwrap();
        for root in [&first_merge, &second_merge] {
            fs::create_dir_all(event_dir(root)).unwrap();
        }
        for (root, files) in [
            (
                &first_merge,
                ["seed.jsonl", "desktop.jsonl", "laptop.jsonl"],
            ),
            (
                &second_merge,
                ["laptop.jsonl", "desktop.jsonl", "seed.jsonl"],
            ),
        ] {
            for file in files {
                let source_root = if file == "laptop.jsonl" {
                    &laptop_root
                } else {
                    &desktop_root
                };
                fs::copy(
                    event_dir(source_root).join(file),
                    event_dir(root).join(file),
                )
                .unwrap();
            }
        }
        let first = load(&first_merge).unwrap();
        let second = load(&second_merge).unwrap();
        assert_eq!(first.media, second.media);
        assert_eq!(first.conflicts, second.conflicts);
        assert_eq!(first.media.len(), 2);
        let merged = first.media.iter().find(|media| media.id == "m").unwrap();
        assert_eq!(merged.tags, vec!["Night", "Track"]);
        assert_eq!(merged.project_ids, vec!["project-a", "project-b"]);
        for root in [desktop_root, laptop_root, first_merge, second_merge] {
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn duplicate_delivery_is_idempotent() {
        let event = envelope(
            "desktop",
            1,
            CatalogueEvent::MediaRegistered {
                media: item("m", "a.mp4", "mp4"),
            },
        );
        let catalogue = rebuild(EventRead {
            events: vec![event.clone(), event],
            ..Default::default()
        });
        assert_eq!(catalogue.media.len(), 1);
    }

    #[test]
    fn divergent_duplicate_event_ids_are_deterministic_and_reported() {
        let mut first = envelope(
            "device-a",
            1,
            CatalogueEvent::MediaRegistered {
                media: item("m", "clip.mp4", "mp4"),
            },
        );
        first.event_id = "duplicate-id".into();
        let mut changed = item("m", "clip.mp4", "mp4");
        changed.notes = "different".into();
        let mut second = envelope(
            "device-b",
            1,
            CatalogueEvent::MediaRegistered { media: changed },
        );
        second.event_id = "duplicate-id".into();
        let forward = rebuild(EventRead {
            events: vec![first.clone(), second.clone()],
            ..Default::default()
        });
        let reverse = rebuild(EventRead {
            events: vec![second, first],
            ..Default::default()
        });
        assert_eq!(forward.media, reverse.media);
        assert!(forward
            .issues
            .iter()
            .any(|issue| issue.code == "duplicate-event-conflict"));
        assert!(reverse
            .issues
            .iter()
            .any(|issue| issue.code == "duplicate-event-conflict"));
    }

    #[test]
    fn concurrent_notes_are_retained_as_conflict_despite_clock_order() {
        let registered = envelope(
            "seed",
            1,
            CatalogueEvent::MediaRegistered {
                media: item("m", "a.mp4", "mp4"),
            },
        );
        let mut desktop = envelope(
            "desktop",
            1,
            CatalogueEvent::MediaNotesSet {
                media_id: "m".into(),
                value: "desktop".into(),
            },
        );
        desktop.occurred_at = "2030-01-01T00:00:00Z".into();
        let mut laptop = envelope(
            "laptop",
            1,
            CatalogueEvent::MediaNotesSet {
                media_id: "m".into(),
                value: "laptop".into(),
            },
        );
        laptop.occurred_at = "2020-01-01T00:00:00Z".into();
        let catalogue = rebuild(EventRead {
            events: vec![laptop, registered, desktop],
            ..Default::default()
        });
        assert_eq!(catalogue.conflicts.len(), 1);
        assert_eq!(catalogue.conflicts[0].values.len(), 2);
    }

    #[test]
    fn manual_metadata_events_do_not_clear_integrity_warnings() {
        let mut changed = item("m", "a.mp4", "mp4");
        changed.needs_review = true;
        let catalogue = rebuild(EventRead {
            events: vec![
                envelope(
                    "desktop",
                    1,
                    CatalogueEvent::MediaRegistered { media: changed },
                ),
                envelope(
                    "desktop",
                    2,
                    CatalogueEvent::MediaTagsAdded {
                        media_id: "m".into(),
                        values: vec!["Track".into()],
                    },
                ),
                envelope(
                    "desktop",
                    3,
                    CatalogueEvent::MediaNotesSet {
                        media_id: "m".into(),
                        value: "Keep warning".into(),
                    },
                ),
            ],
            ..Default::default()
        });
        assert!(catalogue.media[0].needs_review);
        assert_eq!(catalogue.media[0].tags, vec!["Track"]);
        assert_eq!(catalogue.media[0].notes, "Keep warning");
    }

    #[test]
    fn integrity_evidence_is_retained_independently_per_device() {
        let registered = envelope(
            "seed",
            1,
            CatalogueEvent::MediaRegistered {
                media: item("m", "clip.mp4", "mp4"),
            },
        );
        let changed = envelope(
            "desktop",
            1,
            CatalogueEvent::MediaEvidenceObserved {
                media_id: "m".into(),
                observed_size: 2,
                observed_modified_millis: Some(20),
                observed_sha256: Some("changed".into()),
                needs_review: true,
            },
        );
        let verified = envelope(
            "laptop",
            1,
            CatalogueEvent::MediaEvidenceObserved {
                media_id: "m".into(),
                observed_size: 1,
                observed_modified_millis: Some(10),
                observed_sha256: Some("hash-m".into()),
                needs_review: false,
            },
        );
        let catalogue = rebuild(EventRead {
            events: vec![verified, changed, registered],
            ..Default::default()
        });
        let evidence = &catalogue.media[0].integrity_by_device;
        assert!(evidence["desktop"].needs_review);
        assert!(!evidence["laptop"].needs_review);
        assert_eq!(
            evidence["laptop"].observed_sha256.as_deref(),
            Some("hash-m")
        );
    }

    #[test]
    fn pairing_requires_same_directory_and_unambiguous_audio() {
        let mut catalogue = Catalogue {
            media: vec![
                item("v1", "2026-10-21/DJI_0001.mp4", "mp4"),
                item("a1", "2026-10-21/DJI_0001.wav", "wav"),
                item("v2", "2026-10-22/DJI_0001.mp4", "mp4"),
                item("a2", "2026-10-22/DJI_0001.wav", "wav"),
            ],
            ..Default::default()
        };
        pair_audio(&mut catalogue);
        assert_eq!(
            catalogue
                .media
                .iter()
                .find(|item| item.id == "v1")
                .unwrap()
                .attached_audio_id
                .as_deref(),
            Some("a1")
        );
        assert_eq!(
            catalogue
                .media
                .iter()
                .find(|item| item.id == "v2")
                .unwrap()
                .attached_audio_id
                .as_deref(),
            Some("a2")
        );
        catalogue
            .media
            .push(item("a3", "2026-10-22/DJI_0001.wav", "wav"));
        pair_audio(&mut catalogue);
        assert!(catalogue
            .media
            .iter()
            .find(|item| item.id == "v2")
            .unwrap()
            .attached_audio_id
            .is_none());
    }

    #[test]
    fn cache_references_reject_traversal_and_absolute_paths() {
        assert!(safe_cache_reference("item.jpg"));
        assert!(!safe_cache_reference("../item.jpg"));
        assert!(!safe_cache_reference("C:\\cache\\item.jpg"));
        assert!(!safe_cache_reference("folder/item.jpg"));
    }

    #[test]
    fn malformed_and_newer_records_do_not_hide_valid_events() {
        let root = temporary("validation");
        let events = event_dir(&root);
        fs::create_dir_all(&events).unwrap();
        let valid = envelope(
            "desktop",
            1,
            CatalogueEvent::MediaRegistered {
                media: item("m", "a.mp4", "mp4"),
            },
        );
        let newer = serde_json::json!({
            "schemaVersion": 99,
            "eventId": "future",
            "deviceId": "desktop",
            "sequence": 2,
            "occurredAt": "2026-10-22T00:00:00Z",
            "observed": {},
            "event": {"type": "future.event"}
        });
        fs::write(
            events.join("desktop.jsonl"),
            format!(
                "{}\n{{bad json}}\n{}\n",
                serde_json::to_string(&valid).unwrap(),
                newer
            ),
        )
        .unwrap();
        fs::write(events.join("desktop.sync-conflict-1.jsonl"), "ignored\n").unwrap();
        let catalogue = load(&root).unwrap();
        assert_eq!(catalogue.media.len(), 1);
        assert!(catalogue
            .issues
            .iter()
            .any(|issue| issue.code == "malformed-event"));
        assert!(catalogue
            .issues
            .iter()
            .any(|issue| issue.code == "unsupported-schema"));
        assert!(catalogue
            .issues
            .iter()
            .any(|issue| issue.code == "syncthing-conflict-copy"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn migration_preserves_legacy_ids_and_is_idempotent() {
        let root = temporary("migration");
        let legacy = Catalogue {
            media: vec![item("legacy-media", "2026-10-22/clip.mp4", "mp4")],
            projects: vec![ProjectRecord {
                id: "legacy-project".into(),
                name: "Project".into(),
                working_directory: ".".into(),
                media_ids: vec!["legacy-media".into()],
                notes: "note".into(),
                tags: vec!["tag".into()],
                created_at: "2026-10-22T00:00:00Z".into(),
            }],
            folders: vec![FolderRecord {
                id: "legacy-folder".into(),
                relative_path: "2026-10-22".into(),
                display_name: "Day".into(),
                tags: vec!["Track".into()],
                notes: Some("note".into()),
            }],
            ..Default::default()
        };
        fs::write(
            root.join("catalogue.json"),
            serde_json::to_vec_pretty(&legacy).unwrap(),
        )
        .unwrap();
        migrate_legacy_snapshot(&root, "device-a").unwrap();
        let first = fs::read(event_dir(&root).join("migration-v2.jsonl")).unwrap();
        migrate_legacy_snapshot(&root, "device-a").unwrap();
        assert_eq!(
            first,
            fs::read(event_dir(&root).join("migration-v2.jsonl")).unwrap()
        );
        let rebuilt = load(&root).unwrap();
        assert_eq!(rebuilt.media[0].id, "legacy-media");
        assert_eq!(rebuilt.projects[0].id, "legacy-project");
        assert_eq!(rebuilt.folders[0].id, "legacy-folder");
        assert!(root.join("catalogue.pre-events-v1.backup.json").is_file());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn migration_recovers_when_interrupted_after_writing_events() {
        let root = temporary("migration-restart");
        let original = serde_json::to_vec_pretty(&Catalogue {
            media: vec![item("legacy-media", "2026-10-22/clip.mp4", "mp4")],
            ..Default::default()
        })
        .unwrap();
        fs::write(root.join("catalogue.json"), &original).unwrap();
        migrate_legacy_snapshot(&root, "desktop").unwrap();
        let events = fs::read(event_dir(&root).join("migration-v2.jsonl")).unwrap();
        fs::remove_file(root.join("migration-v2.json")).unwrap();
        migrate_legacy_snapshot(&root, "desktop").unwrap();
        assert_eq!(
            fs::read(event_dir(&root).join("migration-v2.jsonl")).unwrap(),
            events
        );
        assert_eq!(
            fs::read(root.join("catalogue.pre-events-v1.backup.json")).unwrap(),
            original
        );
        assert_eq!(load(&root).unwrap().media[0].id, "legacy-media");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn late_legacy_snapshot_is_not_blocked_by_an_empty_initialization() {
        let root = temporary("late-legacy");
        migrate_legacy_snapshot(&root, "device-a").unwrap();
        assert!(!root.join("migration-v2.json").exists());
        let legacy = Catalogue {
            media: vec![item("late", "2026-10-24/late.mp4", "mp4")],
            ..Default::default()
        };
        fs::write(
            root.join("catalogue.json"),
            serde_json::to_vec_pretty(&legacy).unwrap(),
        )
        .unwrap();
        migrate_legacy_snapshot(&root, "device-b").unwrap();
        assert_eq!(load(&root).unwrap().media[0].id, "late");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn competing_devices_produce_one_identical_migration_representation() {
        let root = temporary("competing-migration");
        let legacy = Catalogue {
            media: vec![item("shared", "2026-10-24/shared.mp4", "mp4")],
            ..Default::default()
        };
        fs::write(
            root.join("catalogue.json"),
            serde_json::to_vec_pretty(&legacy).unwrap(),
        )
        .unwrap();
        migrate_legacy_snapshot(&root, "device-a").unwrap();
        let first = fs::read(event_dir(&root).join("migration-v2.jsonl")).unwrap();
        migrate_legacy_snapshot(&root, "device-b").unwrap();
        assert_eq!(
            fs::read(event_dir(&root).join("migration-v2.jsonl")).unwrap(),
            first
        );
        assert_eq!(load(&root).unwrap().media.len(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn event_signature_changes_for_remote_metadata_only_events() {
        let root = temporary("metadata-signature");
        let media = item("shared", "2026-10-24/shared.mp4", "mp4");
        append(
            &root,
            "device-a",
            vec![CatalogueEvent::MediaRegistered {
                media: media.clone(),
            }],
        )
        .unwrap();
        let registered = event_signature(&root).unwrap();
        assert_eq!(registered, event_signature(&root).unwrap());

        append(
            &root,
            "device-b",
            vec![CatalogueEvent::MediaTagsAdded {
                media_id: media.id.clone(),
                values: vec!["Remote tag".into()],
            }],
        )
        .unwrap();
        let tagged = event_signature(&root).unwrap();
        assert_ne!(tagged, registered);

        append(
            &root,
            "device-b",
            vec![CatalogueEvent::MediaProjectsAdded {
                media_id: media.id,
                values: vec!["remote-project".into()],
            }],
        )
        .unwrap();
        assert_ne!(event_signature(&root).unwrap(), tagged);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn append_refuses_a_truncated_owned_stream() {
        let root = temporary("truncated");
        fs::create_dir_all(event_dir(&root)).unwrap();
        fs::write(event_dir(&root).join("desktop.jsonl"), "{\"partial\":true}").unwrap();
        let error = append(
            &root,
            "desktop",
            vec![CatalogueEvent::MediaRegistered {
                media: item("m", "a.mp4", "mp4"),
            }],
        )
        .unwrap_err();
        assert!(error.contains("truncated"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn truncated_final_record_is_reported_but_never_replayed() {
        let root = temporary("truncated-read");
        fs::create_dir_all(event_dir(&root)).unwrap();
        let valid = EventEnvelope {
            schema_version: EVENT_SCHEMA_VERSION,
            event_id: "committed".into(),
            device_id: "desktop".into(),
            sequence: 1,
            occurred_at: "2026-10-22T00:00:00Z".into(),
            observed: BTreeMap::new(),
            event: CatalogueEvent::MediaRegistered {
                media: item("committed", "committed.mp4", "mp4"),
            },
        };
        let partial = EventEnvelope {
            event_id: "uncommitted".into(),
            sequence: 2,
            event: CatalogueEvent::MediaRegistered {
                media: item("uncommitted", "uncommitted.mp4", "mp4"),
            },
            ..valid.clone()
        };
        fs::write(
            event_dir(&root).join("desktop.jsonl"),
            format!(
                "{}\n{}",
                serde_json::to_string(&valid).unwrap(),
                serde_json::to_string(&partial).unwrap()
            ),
        )
        .unwrap();
        let catalogue = load(&root).unwrap();
        assert_eq!(catalogue.media.len(), 1);
        assert_eq!(catalogue.media[0].id, "committed");
        assert!(catalogue
            .issues
            .iter()
            .any(|issue| issue.code == "truncated-stream"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_set_and_update_events_preserve_exact_final_state() {
        let root = temporary("legacy-replacements");
        fs::create_dir_all(event_dir(&root)).unwrap();
        let mut initial = item("m", "old.mp4", "mp4");
        initial.tags = vec!["remove-me".into(), "keep".into()];
        initial.project_ids = vec!["old-project".into()];
        let mut updated = initial.clone();
        updated.relative_path = "new.mp4".into();
        updated.tags = vec!["keep".into()];
        updated.project_ids.clear();
        updated.notes = "final note".into();
        let records = vec![
            serde_json::json!({"schemaVersion":1,"eventId":"one","deviceId":"legacy","sequence":1,"type":"media.registered","payload":initial}),
            serde_json::json!({"schemaVersion":1,"eventId":"two","deviceId":"legacy","sequence":2,"type":"media.tags-set","payload":{"mediaId":"m","tags":["keep"]}}),
            serde_json::json!({"schemaVersion":1,"eventId":"three","deviceId":"legacy","sequence":3,"type":"media.projects-set","payload":{"mediaId":"m","projectIds":[]}}),
        ];
        let content = records
            .iter()
            .map(|record| serde_json::to_string(record).unwrap())
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        fs::write(event_dir(&root).join("legacy.jsonl"), content).unwrap();
        let replaced = load(&root).unwrap();
        assert_eq!(replaced.media[0].tags, vec!["keep"]);
        assert!(replaced.media[0].project_ids.is_empty());
        let update = serde_json::json!({"schemaVersion":1,"eventId":"four","deviceId":"legacy","sequence":4,"type":"media.updated","payload":updated});
        let mut stream = OpenOptions::new()
            .append(true)
            .open(event_dir(&root).join("legacy.jsonl"))
            .unwrap();
        writeln!(stream, "{}", serde_json::to_string(&update).unwrap()).unwrap();
        let catalogue = load(&root).unwrap();
        assert_eq!(catalogue.media[0].relative_path, "new.mp4");
        assert_eq!(catalogue.media[0].notes, "final note");
        assert_eq!(catalogue.media[0].tags, vec!["keep"]);
        assert!(catalogue.media[0].project_ids.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn incomplete_migration_marker_is_not_accepted_as_proof() {
        let root = temporary("incomplete-marker");
        fs::write(
            root.join("migration-v2.json"),
            br#"{"schemaVersion":2,"source":"catalogue.json","sourceSha256":"44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a","backup":"catalogue.pre-events-v1.backup.json","eventStream":"migration-v2.jsonl"}"#,
        )
        .unwrap();
        fs::write(root.join("catalogue.pre-events-v1.backup.json"), b"{}").unwrap();
        let error = migrate_legacy_snapshot(&root, "desktop").unwrap_err();
        assert!(error.contains("event stream is missing"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn partially_arrived_migration_stream_is_preserved_and_rejected() {
        let root = temporary("partial-migration-stream");
        let legacy = Catalogue {
            media: vec![item("legacy", "2026-10-24/legacy.mp4", "mp4")],
            ..Default::default()
        };
        let backup = serde_json::to_vec_pretty(&legacy).unwrap();
        fs::write(
            root.join("catalogue.pre-events-v1.backup.json"),
            &backup,
        )
        .unwrap();
        fs::create_dir_all(event_dir(&root)).unwrap();
        let partial = br#"{"schemaVersion":2"#;
        fs::write(event_dir(&root).join("migration-v2.jsonl"), partial).unwrap();
        fs::write(
            root.join("migration-v2.json"),
            serde_json::to_vec(&serde_json::json!({
                "schemaVersion": 2,
                "source": "catalogue.json",
                "sourceSha256": sha256_bytes(&backup),
                "backup": "catalogue.pre-events-v1.backup.json",
                "eventStream": "migration-v2.jsonl"
            }))
            .unwrap(),
        )
        .unwrap();

        let error = migrate_legacy_snapshot(&root, "desktop").unwrap_err();
        assert!(error.contains("truncated"));
        assert_eq!(
            fs::read(event_dir(&root).join("migration-v2.jsonl")).unwrap(),
            partial
        );
        assert_eq!(
            fs::read(root.join("catalogue.pre-events-v1.backup.json")).unwrap(),
            backup
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn damaged_legacy_snapshot_stops_without_creating_migration_state() {
        let root = temporary("damaged-migration");
        fs::write(root.join("catalogue.json"), br#"{"unexpected":true}"#).unwrap();
        assert!(migrate_legacy_snapshot(&root, "desktop").is_err());
        assert!(!root.join("migration-v2.json").exists());
        assert!(!root.join("catalogue.pre-events-v1.backup.json").exists());
        assert_eq!(
            fs::read(root.join("catalogue.json")).unwrap(),
            br#"{"unexpected":true}"#
        );
        fs::remove_dir_all(root).unwrap();
    }
}
