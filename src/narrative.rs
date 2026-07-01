use crate::dependency_graph::{
    completed_blocks, open_blocks, resolved_blockers, unresolved_blockers,
};
use crate::model::{SlateWorkFile, SlateWorkRecord, WorkStatus};
use crate::slate_body::default_slate_work_body;
use crate::text::{dedup, extract_section_paragraph, extract_title};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub(crate) fn extract_first_paragraph(text: &str) -> Option<String> {
    let mut lines = Vec::new();
    let mut seen_content = false;
    let mut in_fence = false;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence || trimmed.starts_with('#') {
            if seen_content && !lines.is_empty() {
                break;
            }
            continue;
        }
        if trimmed.is_empty() {
            if seen_content && !lines.is_empty() {
                break;
            }
            continue;
        }
        if trimmed.starts_with('-') {
            continue;
        }
        seen_content = true;
        lines.push(trimmed.to_string());
    }

    if lines.is_empty() {
        None
    } else {
        Some(lines.join(" "))
    }
}

pub(crate) fn effective_work_body(record: &SlateWorkRecord) -> String {
    if record.body.trim().is_empty() {
        default_slate_work_body(&record.work)
    } else {
        record.body.clone()
    }
}

pub(crate) fn work_narrative_summary(record: &SlateWorkRecord) -> String {
    let body = effective_work_body(record);
    let story = extract_section_paragraph(&body, "## Story")
        .or_else(|| extract_section_paragraph(&body, "## Problem"))
        .or_else(|| extract_section_paragraph(&body, "## Context"))
        .or_else(|| extract_first_paragraph(&body))
        .unwrap_or_else(|| record.work.human_request.clone());
    let why = extract_section_paragraph(&body, "## Why")
        .or_else(|| extract_section_paragraph(&body, "## Rationale"))
        .or_else(|| extract_section_paragraph(&body, "## User Alignment"))
        .unwrap_or_else(|| record.work.user_alignment.clone());
    let direction = extract_section_paragraph(&body, "## Direction")
        .or_else(|| extract_section_paragraph(&body, "## Next"))
        .unwrap_or_else(|| work_direction_from_fields(&record.work));

    [
        ("Story", story.trim()),
        ("Why", why.trim()),
        ("Direction", direction.trim()),
    ]
    .into_iter()
    .filter(|(_, value)| !value.is_empty())
    .map(|(label, value)| format!("{}: {}", label, value))
    .collect::<Vec<_>>()
    .join("\n")
}

pub(crate) fn work_direction_from_fields(work: &SlateWorkFile) -> String {
    let blocked_by = if work.blocked_by.is_empty() {
        "nothing".to_string()
    } else {
        work.blocked_by.join(", ")
    };
    let blocks = if work.blocks.is_empty() {
        "nothing recorded".to_string()
    } else {
        work.blocks.join(", ")
    };
    format!("Blocked by {}; blocks {}.", blocked_by, blocks)
}

pub(crate) fn markdown_link_targets(text: &str) -> Vec<String> {
    let mut targets = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("](") {
        let after_start = &rest[start + 2..];
        let Some(end) = after_start.find(')') else {
            break;
        };
        targets.push(after_start[..end].to_string());
        rest = &after_start[end + 1..];
    }
    targets
}

pub(crate) fn clean_path_ref(raw: &str) -> Option<String> {
    let cleaned = raw
        .trim()
        .trim_matches(|c: char| {
            matches!(
                c,
                '`' | '"' | '\'' | ',' | ';' | ':' | '.' | ')' | '(' | '[' | ']' | '<' | '>'
            )
        })
        .trim_start_matches("./")
        .to_string();

    if cleaned.is_empty()
        || cleaned.starts_with('/')
        || cleaned.contains("://")
        || cleaned.contains("..")
    {
        return None;
    }

    let is_doc = cleaned.ends_with(".md")
        || cleaned.ends_with(".allium")
        || cleaned.ends_with(".toml")
        || cleaned.ends_with(".rs");
    if is_doc {
        Some(cleaned)
    } else {
        None
    }
}

pub(crate) fn extract_path_refs(text: &str) -> Vec<String> {
    let mut refs = markdown_link_targets(text);
    refs.extend(text.split_whitespace().map(|token| token.to_string()));
    dedup(
        refs.into_iter()
            .filter_map(|raw| clean_path_ref(&raw))
            .collect(),
    )
}

pub(crate) fn collect_work_context_refs(record: &SlateWorkRecord) -> Vec<String> {
    let mut refs = Vec::new();
    refs.extend(record.work.allium_anchors.clone());
    refs.extend(record.work.belief_refs.clone());
    refs.extend(extract_path_refs(&record.work.human_request));
    refs.extend(extract_path_refs(&record.work.user_alignment));
    refs.extend(
        record
            .work
            .implementation_plan
            .iter()
            .flat_map(|item| extract_path_refs(item)),
    );
    refs.extend(
        record
            .work
            .proof_plan
            .iter()
            .flat_map(|item| extract_path_refs(item)),
    );
    refs.extend(
        record
            .work
            .closure_evidence
            .iter()
            .flat_map(|item| extract_path_refs(item)),
    );
    refs.extend(extract_path_refs(&effective_work_body(record)));
    dedup(
        refs.into_iter()
            .filter_map(|raw| clean_path_ref(&raw))
            .collect(),
    )
}

pub(crate) fn context_digest(root: &Path, path: &str) -> Option<String> {
    let full_path = root.join(path);
    if !full_path.is_file() {
        return None;
    }
    let raw = fs::read_to_string(&full_path).ok()?;
    let title = extract_title(&raw).unwrap_or_else(|| path.to_string());
    let summary = extract_section_paragraph(&raw, "## Purpose")
        .or_else(|| extract_section_paragraph(&raw, "## Summary"))
        .or_else(|| extract_first_paragraph(&raw))
        .unwrap_or_else(|| raw.lines().take(5).collect::<Vec<_>>().join(" "));
    let summary = summary.chars().take(700).collect::<String>();
    Some(format!("{} — {} — {}", path, title, summary.trim()))
}

pub(crate) fn work_narrative_context(root: &Path, record: &SlateWorkRecord) -> Vec<String> {
    collect_work_context_refs(record)
        .into_iter()
        .filter_map(|path| context_digest(root, &path))
        .collect()
}

pub(crate) fn work_read_first(root: &Path, record: &SlateWorkRecord) -> Vec<String> {
    let mut read_first = vec![
        "layer/allium/".to_string(),
        "layer/surface/epistemic/beliefs/".to_string(),
        "layer/core/".to_string(),
        record.path.clone(),
    ];
    if let Some(body_path) = &record.body_path {
        read_first.push(body_path.clone());
    }
    read_first.extend(
        collect_work_context_refs(record)
            .into_iter()
            .filter(|path| root.join(path).exists()),
    );
    dedup(read_first)
}

pub(crate) fn dependency_direction_summary(
    work: &SlateWorkFile,
    status_map: &HashMap<String, WorkStatus>,
) -> String {
    let unresolved = unresolved_blockers(work, status_map);
    let resolved = resolved_blockers(work, status_map);
    let open_blocks = open_blocks(work, status_map);
    let completed_blocks = completed_blocks(work, status_map);

    format!(
        "blocked_by unresolved=[{}] resolved=[{}]; blocks open=[{}] complete=[{}]",
        unresolved.join(", "),
        resolved.join(", "),
        open_blocks.join(", "),
        completed_blocks.join(", ")
    )
}
