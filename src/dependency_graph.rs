use crate::model::{SlateWorkFile, SlateWorkRecord, WorkStatus};
use crate::store::{append_slate_event, load_slate_work, slate_work_path, write_slate_work_file};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub(crate) fn is_terminal_work_status(status: WorkStatus) -> bool {
    status.is_terminal()
}

pub(crate) fn push_unique(values: &mut Vec<String>, value: String) {
    if !value.trim().is_empty() && !values.contains(&value) {
        values.push(value);
    }
}

pub(crate) fn normalize_slate_dependency_edges(records: &mut [SlateWorkRecord]) {
    let mut blocked_by_edges = Vec::new();
    let mut blocks_edges = Vec::new();

    for record in records.iter() {
        for blocker in &record.work.blocked_by {
            blocked_by_edges.push((blocker.clone(), record.work.id.clone()));
        }
        for blocked in &record.work.blocks {
            blocks_edges.push((record.work.id.clone(), blocked.clone()));
        }
    }

    for (blocker, blocked) in blocked_by_edges.iter().chain(blocks_edges.iter()) {
        let blocker_exists = records.iter().any(|record| record.work.id == *blocker);
        let blocked_exists = records.iter().any(|record| record.work.id == *blocked);
        if !blocker_exists || !blocked_exists {
            continue;
        }

        if let Some(blocker_record) = records.iter_mut().find(|record| record.work.id == *blocker) {
            push_unique(&mut blocker_record.work.blocks, blocked.clone());
        }
        if let Some(blocked_record) = records.iter_mut().find(|record| record.work.id == *blocked) {
            push_unique(&mut blocked_record.work.blocked_by, blocker.clone());
        }
    }

    for record in records {
        record.work.blocked_by.sort();
        record.work.blocked_by.dedup();
        record.work.blocks.sort();
        record.work.blocks.dedup();
    }
}

pub(crate) fn slate_status_map(records: &[SlateWorkRecord]) -> HashMap<String, WorkStatus> {
    records
        .iter()
        .map(|record| (record.work.id.clone(), record.work.status))
        .collect()
}

pub(crate) fn unresolved_blockers(
    work: &SlateWorkFile,
    status_map: &HashMap<String, WorkStatus>,
) -> Vec<String> {
    work.blocked_by
        .iter()
        .filter(|id| {
            status_map
                .get(*id)
                .map(|status| !is_terminal_work_status(*status))
                .unwrap_or(true)
        })
        .cloned()
        .collect()
}

pub(crate) fn resolved_blockers(
    work: &SlateWorkFile,
    status_map: &HashMap<String, WorkStatus>,
) -> Vec<String> {
    work.blocked_by
        .iter()
        .filter(|id| {
            status_map
                .get(*id)
                .map(|status| is_terminal_work_status(*status))
                .unwrap_or(false)
        })
        .cloned()
        .collect()
}

pub(crate) fn open_blocks(
    work: &SlateWorkFile,
    status_map: &HashMap<String, WorkStatus>,
) -> Vec<String> {
    work.blocks
        .iter()
        .filter(|id| {
            status_map
                .get(*id)
                .map(|status| !is_terminal_work_status(*status))
                .unwrap_or(true)
        })
        .cloned()
        .collect()
}

pub(crate) fn completed_blocks(
    work: &SlateWorkFile,
    status_map: &HashMap<String, WorkStatus>,
) -> Vec<String> {
    work.blocks
        .iter()
        .filter(|id| {
            status_map
                .get(*id)
                .map(|status| is_terminal_work_status(*status))
                .unwrap_or(false)
        })
        .cloned()
        .collect()
}

pub(crate) fn dependency_warnings(
    work: &SlateWorkFile,
    status_map: &HashMap<String, WorkStatus>,
) -> Vec<String> {
    let unresolved = unresolved_blockers(work, status_map);
    if work.status == WorkStatus::Blocked && !work.blocked_by.is_empty() && unresolved.is_empty() {
        vec![format!(
            "status is blocked but all blockers are terminal: {}",
            work.blocked_by.join(", ")
        )]
    } else {
        Vec::new()
    }
}

pub(crate) fn reconcile_slate_dependencies(root: &Path, cause: &str) -> Result<(), String> {
    let mut records = load_slate_work(root)?;
    normalize_slate_dependency_edges(&mut records);
    let status_map = slate_status_map(&records);

    for record in records {
        let path = slate_work_path(root, &record.work.id);
        let raw =
            fs::read_to_string(&path).map_err(|e| format!("read {}: {}", path.display(), e))?;
        let before: SlateWorkFile =
            toml::from_str(&raw).map_err(|e| format!("parse {}: {}", path.display(), e))?;
        let mut next = record.work.clone();
        let mut changed = before.blocked_by != next.blocked_by || before.blocks != next.blocks;
        let mut unblocked = false;

        if next.status == WorkStatus::Blocked
            && !next.blocked_by.is_empty()
            && unresolved_blockers(&next, &status_map).is_empty()
        {
            next.status = WorkStatus::Active;
            next.block_reason = None;
            unblocked = true;
            changed = true;
        }

        if changed {
            let saved = write_slate_work_file(root, &mut next)?;
            append_slate_event(
                root,
                &saved.work.id,
                "dependencies-reconciled",
                json!({
                    "cause": cause,
                    "unblocked": unblocked,
                    "blocked_by": saved.work.blocked_by,
                    "blocks": saved.work.blocks,
                }),
            )?;
        }
    }

    Ok(())
}
