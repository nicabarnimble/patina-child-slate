use crate::dependency_graph::normalize_slate_dependency_edges;
use crate::model::{SlateWorkFile, SlateWorkRecord};
use crate::runtime::to_repo_relative;
use crate::slate_body::{ensure_slate_work_body, read_slate_work_body};
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) const SLATE_WORK_DIR: &str = "layer/slate/work";
pub(crate) const SLATE_EVENTS_PATH: &str = "layer/slate/events.jsonl";
pub(crate) fn slate_work_dir(root: &Path) -> PathBuf {
    root.join(SLATE_WORK_DIR)
}

pub(crate) fn slate_work_path(root: &Path, id: &str) -> PathBuf {
    slate_work_dir(root).join(id).join("work.toml")
}

pub(crate) fn validate_slate_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(format!("invalid Slate work id '{}': use kebab-case", id));
    }
    Ok(())
}

pub(crate) fn load_slate_work(root: &Path) -> Result<Vec<SlateWorkRecord>, String> {
    let dir = slate_work_dir(root);
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    fn walk(root: &Path, dir: &Path, out: &mut Vec<SlateWorkRecord>) -> Result<(), String> {
        for entry in fs::read_dir(dir).map_err(|e| format!("read {}: {}", dir.display(), e))? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, out)?;
                continue;
            }
            if path.file_name().and_then(|name| name.to_str()) != Some("work.toml") {
                continue;
            }
            let raw =
                fs::read_to_string(&path).map_err(|e| format!("read {}: {}", path.display(), e))?;
            let work: SlateWorkFile =
                toml::from_str(&raw).map_err(|e| format!("parse {}: {}", path.display(), e))?;
            if work.id.is_empty() {
                return Err(format!("Slate work file {} has empty id", path.display()));
            }
            let (body_path, body) = read_slate_work_body(root, &path, &work)?;
            out.push(SlateWorkRecord {
                work,
                path: to_repo_relative(root, &path),
                body_path,
                body,
            });
        }
        Ok(())
    }

    walk(root, &dir, &mut records)?;
    normalize_slate_dependency_edges(&mut records);
    records.sort_by(|a, b| {
        a.work
            .status
            .cmp(&b.work.status)
            .then(a.work.kind.cmp(&b.work.kind))
            .then(a.work.id.cmp(&b.work.id))
    });
    Ok(records)
}

pub(crate) fn find_slate_work<'a>(
    records: &'a [SlateWorkRecord],
    id: &str,
) -> Result<&'a SlateWorkRecord, String> {
    records
        .iter()
        .find(|record| record.work.id == id)
        .ok_or_else(|| format!("Slate work '{}' not found", id))
}

pub(crate) fn timestamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

pub(crate) fn write_slate_work_file(
    root: &Path,
    work: &mut SlateWorkFile,
) -> Result<SlateWorkRecord, String> {
    validate_slate_id(&work.id)?;
    let now = timestamp();
    if work.created_at.is_none() {
        work.created_at = Some(now.clone());
    }
    work.updated_at = Some(now);

    let path = slate_work_path(root, &work.id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create {}: {}", parent.display(), e))?;
    }
    let content = toml::to_string_pretty(work).map_err(|e| e.to_string())?;
    fs::write(&path, content).map_err(|e| format!("write {}: {}", path.display(), e))?;
    let (body_path, body) = read_slate_work_body(root, &path, work)?;
    Ok(SlateWorkRecord {
        work: work.clone(),
        path: to_repo_relative(root, &path),
        body_path,
        body,
    })
}

pub(crate) fn append_slate_event(
    root: &Path,
    work_id: &str,
    event_type: &str,
    payload: serde_json::Value,
) -> Result<(), String> {
    let path = root.join(SLATE_EVENTS_PATH);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create {}: {}", parent.display(), e))?;
    }
    let event = serde_json::json!({
        "work_id": work_id,
        "event_type": event_type,
        "payload": payload,
        "created_at": timestamp(),
    });
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("open {}: {}", path.display(), e))?;
    writeln!(file, "{}", event).map_err(|e| format!("append {}: {}", path.display(), e))
}

pub(crate) fn create_slate_work_file(
    root: &Path,
    work: &mut SlateWorkFile,
) -> Result<SlateWorkRecord, String> {
    validate_slate_id(&work.id)?;
    let path = slate_work_path(root, &work.id);
    if path.exists() {
        return Err(format!("Slate work '{}' already exists", work.id));
    }
    let record = write_slate_work_file(root, work)?;
    ensure_slate_work_body(root, &record.work)?;
    append_slate_event(
        root,
        &record.work.id,
        "created",
        serde_json::json!({"status": record.work.status}),
    )?;
    let records = load_slate_work(root)?;
    find_slate_work(&records, &record.work.id).cloned()
}

pub(crate) fn update_slate_work(
    root: &Path,
    id: &str,
    event_type: &str,
    payload: serde_json::Value,
    mutate: impl FnOnce(&mut SlateWorkFile) -> Result<(), String>,
) -> Result<SlateWorkRecord, String> {
    let mut records = load_slate_work(root)?;
    let mut record = find_slate_work(&records, id)?.clone();
    mutate(&mut record.work)?;
    let saved = write_slate_work_file(root, &mut record.work)?;
    append_slate_event(root, &saved.work.id, event_type, payload)?;
    records.clear();
    Ok(saved)
}

pub(crate) fn load_slate_events(root: &Path, id: &str) -> Result<Vec<serde_json::Value>, String> {
    let path = root.join(SLATE_EVENTS_PATH);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(&path).map_err(|e| format!("read {}: {}", path.display(), e))?;
    let mut events = Vec::new();
    for line in raw.lines().filter(|line| !line.trim().is_empty()) {
        let event: serde_json::Value =
            serde_json::from_str(line).map_err(|e| format!("parse {}: {}", path.display(), e))?;
        if event.get("work_id").and_then(|value| value.as_str()) == Some(id) {
            events.push(event);
        }
    }
    Ok(events)
}
