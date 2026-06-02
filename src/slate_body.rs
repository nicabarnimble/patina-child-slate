use crate::model::{SlateWorkFile, SlateWorkRecord};
use crate::narrative::effective_work_body;
use crate::runtime::to_repo_relative;
use crate::store::{append_slate_event, find_slate_work, load_slate_work, slate_work_dir};
use crate::work_fields::SetWorkOperation;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn slate_work_body_path(root: &Path, id: &str) -> PathBuf {
    slate_work_dir(root).join(id).join("work.md")
}

pub(crate) fn read_slate_work_body(
    root: &Path,
    work_path: &Path,
    work: &SlateWorkFile,
) -> Result<(Option<String>, String), String> {
    let body_path = work_path
        .parent()
        .map(|parent| parent.join("work.md"))
        .unwrap_or_else(|| slate_work_body_path(root, &work.id));
    if !body_path.exists() {
        return Ok((None, String::new()));
    }
    let body = fs::read_to_string(&body_path)
        .map_err(|e| format!("read {}: {}", body_path.display(), e))?;
    Ok((Some(to_repo_relative(root, &body_path)), body))
}

pub(crate) fn default_slate_work_body(work: &SlateWorkFile) -> String {
    let anchors = if work.allium_anchors.is_empty() {
        "- No Allium/context anchors captured yet.".to_string()
    } else {
        work.allium_anchors
            .iter()
            .map(|anchor| format!("- {}", anchor))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let blockers = if work.blocked_by.is_empty() {
        "- None recorded.".to_string()
    } else {
        work.blocked_by
            .iter()
            .map(|id| format!("- {}", id))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let blocks = if work.blocks.is_empty() {
        "- None recorded yet.".to_string()
    } else {
        work.blocks
            .iter()
            .map(|id| format!("- {}", id))
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        "# {}\n\n## Story\n{}\n\n## Why\n{}\n\n## Direction\nBlocked by:\n{}\n\nBlocks:\n{}\n\n## Context\n{}\n\n## Notes\n- Add the narrative reasoning, trade-offs, and links to design/context docs here.\n",
        if work.title.trim().is_empty() {
            &work.id
        } else {
            &work.title
        },
        if work.human_request.trim().is_empty() {
            "No human request captured yet."
        } else {
            work.human_request.trim()
        },
        if work.user_alignment.trim().is_empty() {
            "No user alignment/rationale captured yet."
        } else {
            work.user_alignment.trim()
        },
        blockers,
        blocks,
        anchors,
    )
}

pub(crate) fn write_slate_work_body(root: &Path, id: &str, body: &str) -> Result<String, String> {
    let path = slate_work_body_path(root, id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create {}: {}", parent.display(), e))?;
    }
    fs::write(&path, body).map_err(|e| format!("write {}: {}", path.display(), e))?;
    Ok(to_repo_relative(root, &path))
}

pub(crate) fn ensure_slate_work_body(root: &Path, work: &SlateWorkFile) -> Result<String, String> {
    let path = slate_work_body_path(root, &work.id);
    if !path.exists() {
        write_slate_work_body(root, &work.id, &default_slate_work_body(work))
    } else {
        Ok(to_repo_relative(root, &path))
    }
}

pub(crate) fn update_slate_work_body(
    root: &Path,
    id: &str,
    operation: &SetWorkOperation,
    value: &str,
) -> Result<SlateWorkRecord, String> {
    let records = load_slate_work(root)?;
    let record = find_slate_work(&records, id)?.clone();
    let current = effective_work_body(&record);
    let next = match operation {
        SetWorkOperation::Default | SetWorkOperation::Set => value.trim().to_string(),
        SetWorkOperation::Add => {
            if current.trim().is_empty() {
                value.trim().to_string()
            } else {
                format!("{}\n\n{}", current.trim_end(), value.trim())
            }
        }
        SetWorkOperation::Remove => String::new(),
        SetWorkOperation::Update(_) => return Err(
            "operation not supported for Slate work body; use body:set, body:add, or body:remove"
                .to_string(),
        ),
    };

    if next.is_empty() {
        let path = slate_work_body_path(root, id);
        if path.exists() {
            fs::remove_file(&path).map_err(|e| format!("remove {}: {}", path.display(), e))?;
        }
    } else {
        write_slate_work_body(root, id, &next)?;
    }
    append_slate_event(
        root,
        id,
        "body-updated",
        serde_json::json!({"operation": format!("{:?}", operation)}),
    )?;

    let records = load_slate_work(root)?;
    find_slate_work(&records, id).cloned()
}
