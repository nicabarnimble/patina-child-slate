use crate::dependency_graph::{
    completed_blocks, dependency_warnings, open_blocks, reconcile_slate_dependencies,
    resolved_blockers, slate_status_map, unresolved_blockers,
};
use crate::model::{normalize_slate_kind, SlateWorkRecord, WorkStatus};
use crate::narrative::{
    dependency_direction_summary, work_narrative_context, work_narrative_summary, work_read_first,
};
use crate::runtime::{arg_bool, arg_string, require_id};
use crate::store::{find_slate_work, load_slate_work, timestamp, update_slate_work};
use crate::work_views::{
    closure_gates, normalize_work_items, validate_complete_gate, work_progress, work_state_json,
};
use std::path::Path;

pub(crate) fn work_record_json(
    root: &Path,
    records: &[SlateWorkRecord],
    record: &SlateWorkRecord,
) -> serde_json::Value {
    let status_map = slate_status_map(records);
    serde_json::json!({
        "id": record.work.id,
        "title": record.work.title,
        "kind": record.work.kind,
        "status": record.work.status,
        "human_request": record.work.human_request,
        "user_alignment": record.work.user_alignment,
        "user_value": record.work.user_value,
        "scope": record.work.scope,
        "non_goals": record.work.non_goals,
        "stop_condition": record.work.stop_condition,
        "allium_anchors": record.work.allium_anchors,
        "belief_refs": record.work.belief_refs,
        "implementation_plan": record.work.implementation_plan,
        "proof_plan": record.work.proof_plan,
        "closure_evidence": record.work.closure_evidence,
        "blocked_by": record.work.blocked_by,
        "blocks": record.work.blocks,
        "target": record.work.target,
        "belief_harvest_decision": record.work.belief_harvest_decision,
        "path": record.path,
        "body_path": record.body_path,
        "narrative_summary": work_narrative_summary(record),
        "read_first": work_read_first(root, record),
        "narrative_context": work_narrative_context(root, record),
        "dependencies": {
            "unresolved_blockers": unresolved_blockers(&record.work, &status_map),
            "resolved_blockers": resolved_blockers(&record.work, &status_map),
            "open_blocks": open_blocks(&record.work, &status_map),
            "completed_blocks": completed_blocks(&record.work, &status_map),
            "direction": dependency_direction_summary(&record.work, &status_map),
            "warnings": dependency_warnings(&record.work, &status_map),
        }
    })
}

pub(crate) fn handle_work_list(
    root: &Path,
    args: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Result<serde_json::Value, String> {
    let status_filter = arg_string(args, "status");
    let target_filter = arg_string(args, "target");
    let kind_filter = arg_string(args, "kind");
    let records = load_slate_work(root)?;
    let data = records
        .iter()
        .filter(|record| {
            let status_ok = status_filter
                .as_deref()
                .is_none_or(|expected| record.work.status == expected);
            let target_ok = target_filter
                .as_deref()
                .is_none_or(|expected| record.work.target.as_deref() == Some(expected));
            let kind_ok = kind_filter
                .as_deref()
                .is_none_or(|expected| record.work.kind == normalize_slate_kind(expected));
            status_ok && target_ok && kind_ok
        })
        .map(|record| {
            serde_json::json!({
                "id": record.work.id,
                "status": record.work.status,
                "target": record.work.target,
                "kind": record.work.kind,
                "title": record.work.title,
                "path": record.path,
                "body_path": record.body_path,
                "narrative_summary": work_narrative_summary(record),
            })
        })
        .collect::<Vec<_>>();
    Ok(serde_json::Value::Array(data))
}

pub(crate) fn handle_work_next(
    root: &Path,
    args: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Result<serde_json::Value, String> {
    let kind_filter = arg_string(args, "kind");
    let records = load_slate_work(root)?;
    let status_map = slate_status_map(&records);
    let mut rows = records
        .iter()
        .filter(|record| {
            kind_filter
                .as_deref()
                .is_none_or(|kind| record.work.kind == normalize_slate_kind(kind))
        })
        .filter_map(|record| {
            let unresolved = unresolved_blockers(&record.work, &status_map);
            if !unresolved.is_empty() {
                return None;
            }
            let impact = open_blocks(&record.work, &status_map).len()
                + completed_blocks(&record.work, &status_map).len();
            let (priority, reason) = match record.work.status.as_str() {
                "active" => (1, "Currently active — continue working".to_string()),
                "blocked" if !record.work.blocked_by.is_empty() => (
                    2,
                    "Blockers complete — stale blocked status; resume or reconcile".to_string(),
                ),
                "ready" => (3, "Ready to start".to_string()),
                "blocked" => (4, "Blocked without dependency".to_string()),
                "draft" => (5, "Draft needs intent/proof alignment".to_string()),
                _ => return None,
            };
            Some(serde_json::json!({
                "id": record.work.id,
                "status": record.work.status,
                "reason": reason,
                "priority": priority,
                "impact": impact,
                "target": record.work.target,
                "path": record.path,
                "body_path": record.body_path,
                "narrative_summary": work_narrative_summary(record),
            }))
        })
        .collect::<Vec<_>>();

    rows.sort_by(|a, b| {
        let ap = a
            .get("priority")
            .and_then(|v| v.as_u64())
            .unwrap_or(u64::MAX);
        let bp = b
            .get("priority")
            .and_then(|v| v.as_u64())
            .unwrap_or(u64::MAX);
        let ai = a.get("impact").and_then(|v| v.as_u64()).unwrap_or(0);
        let bi = b.get("impact").and_then(|v| v.as_u64()).unwrap_or(0);
        ap.cmp(&bp).then_with(|| bi.cmp(&ai))
    });
    Ok(serde_json::Value::Array(rows))
}

pub(crate) fn handle_work_check(
    root: &Path,
    args: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Result<serde_json::Value, String> {
    let id = require_id(args, "check")?;
    let records = load_slate_work(root)?;
    let record = find_slate_work(&records, id)?;
    let criteria = normalize_work_items(&record.work.proof_plan);
    let total = criteria.len();
    let checked = criteria.iter().filter(|(_, _, checked)| *checked).count();
    let unchecked = criteria
        .into_iter()
        .filter(|(_, _, checked)| !*checked)
        .map(|(criterion_id, text, _)| serde_json::json!({"id": criterion_id, "text": text}))
        .collect::<Vec<_>>();
    let status_map = slate_status_map(&records);
    Ok(serde_json::json!({
        "work_id": id,
        "total": total,
        "checked": checked,
        "unchecked": unchecked,
        "passed": checked == total && total > 0,
        "dependencies": {
            "unresolved_blockers": unresolved_blockers(&record.work, &status_map),
            "warnings": dependency_warnings(&record.work, &status_map),
        }
    }))
}

pub(crate) fn handle_work_show(
    root: &Path,
    args: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Result<serde_json::Value, String> {
    let id = require_id(args, "show")?;
    let records = load_slate_work(root)?;
    let record = find_slate_work(&records, id)?;
    Ok(work_record_json(root, &records, record))
}

pub(crate) fn build_work_prompt_packet_json(
    root: &Path,
    records: &[SlateWorkRecord],
    record: &SlateWorkRecord,
) -> serde_json::Value {
    let status_map = slate_status_map(records);
    serde_json::json!({
        "work_id": record.work.id,
        "status": record.work.status,
        "title": record.work.title,
        "human_request": record.work.human_request,
        "user_value": record.work.user_value,
        "scope": record.work.scope,
        "non_goals": record.work.non_goals,
        "stop_condition": record.work.stop_condition,
        "narrative_summary": work_narrative_summary(record),
        "narrative_context": work_narrative_context(root, record),
        "read_first": work_read_first(root, record),
        "allium_anchors": record.work.allium_anchors,
        "implementation_plan": record.work.implementation_plan,
        "proof_plan": record.work.proof_plan,
        "belief_refs": record.work.belief_refs,
        "closure_gates": closure_gates(&record.work),
        "blocked_by": record.work.blocked_by,
        "blocks": record.work.blocks,
        "direction": dependency_direction_summary(&record.work, &status_map),
        "dependency_warnings": dependency_warnings(&record.work, &status_map),
        "path": record.path,
        "body_path": record.body_path,
    })
}

pub(crate) fn build_work_handoff_packet_json(
    root: &Path,
    records: &[SlateWorkRecord],
    record: &SlateWorkRecord,
) -> serde_json::Value {
    let status_map = slate_status_map(records);
    let (checked, total, completed_items, open_items) = work_progress(&record.work);
    serde_json::json!({
        "work_id": record.work.id,
        "status": record.work.status,
        "title": record.work.title,
        "narrative_summary": work_narrative_summary(record),
        "narrative_context": work_narrative_context(root, record),
        "progress": {"checked": checked, "total": total},
        "completed_items": completed_items,
        "open_items": open_items,
        "next_steps": record.work.implementation_plan,
        "closure_evidence": record.work.closure_evidence,
        "belief_harvest_decision": record.work.belief_harvest_decision,
        "blocked_by": record.work.blocked_by,
        "blocks": record.work.blocks,
        "direction": dependency_direction_summary(&record.work, &status_map),
        "dependency_warnings": dependency_warnings(&record.work, &status_map),
        "path": record.path,
        "body_path": record.body_path,
    })
}

pub(crate) fn handle_work_prompt(
    root: &Path,
    args: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Result<serde_json::Value, String> {
    let id = require_id(args, "prompt")?;
    let records = load_slate_work(root)?;
    let record = find_slate_work(&records, id)?;
    Ok(build_work_prompt_packet_json(root, &records, record))
}

pub(crate) fn handle_work_handoff(
    root: &Path,
    args: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Result<serde_json::Value, String> {
    let id = require_id(args, "handoff")?;
    let records = load_slate_work(root)?;
    let record = find_slate_work(&records, id)?;
    Ok(build_work_handoff_packet_json(root, &records, record))
}

pub(crate) fn handle_work_packet(
    root: &Path,
    args: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Result<serde_json::Value, String> {
    let id = require_id(args, "packet")?;
    let records = load_slate_work(root)?;
    let record = find_slate_work(&records, id)?;
    Ok(serde_json::json!({
        "prompt": build_work_prompt_packet_json(root, &records, record),
        "handoff": build_work_handoff_packet_json(root, &records, record),
        "state": work_state_json(root, &records, record)?,
    }))
}

pub(crate) fn handle_work_complete(
    root: &Path,
    args: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Result<serde_json::Value, String> {
    let id = require_id(args, "complete")?;
    let force = arg_bool(args, "force", false);
    update_slate_work(
        root,
        id,
        "completed",
        serde_json::json!({"force": force}),
        |work| {
            if !force {
                validate_complete_gate(work)?;
            }
            work.status = WorkStatus::Complete;
            work.closed_at = Some(timestamp());
            Ok(())
        },
    )?;
    reconcile_slate_dependencies(root, "complete")?;
    let records = load_slate_work(root)?;
    let record = find_slate_work(&records, id)?;
    Ok(serde_json::json!({
        "command": "complete",
        "work_id": id,
        "new_status": record.work.status,
        "record": work_record_json(root, &records, record),
    }))
}
