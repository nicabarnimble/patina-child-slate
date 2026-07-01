use crate::dependency_graph::{
    dependency_warnings, resolved_blockers, slate_status_map, unresolved_blockers,
};
use crate::exports;
use crate::model::{SlateWorkFile, SlateWorkRecord};
use crate::narrative::{
    dependency_direction_summary, work_narrative_context, work_narrative_summary, work_read_first,
};
use crate::store::load_slate_events;
use crate::text::slugify;
use serde::Serialize;
use std::path::Path;

pub(crate) fn slate_work_summary(
    record: SlateWorkRecord,
) -> exports::patina::slate::control::WorkSummary {
    exports::patina::slate::control::WorkSummary {
        id: record.work.id,
        title: record.work.title,
        kind: record.work.kind.to_string(),
        status: record.work.status.to_string(),
        path: record.path,
    }
}

pub(crate) fn slate_work_record(
    _root: &Path,
    record: SlateWorkRecord,
) -> exports::patina::slate::control::WorkRecord {
    let narrative_summary = work_narrative_summary(&record);
    exports::patina::slate::control::WorkRecord {
        id: record.work.id,
        title: record.work.title,
        kind: record.work.kind.to_string(),
        status: record.work.status.to_string(),
        human_request: record.work.human_request,
        user_value: record.work.user_value,
        scope: record.work.scope,
        non_goals: record.work.non_goals,
        stop_condition: record.work.stop_condition,
        allium_anchors: record.work.allium_anchors,
        user_alignment: record.work.user_alignment,
        belief_refs: record.work.belief_refs,
        proof_plan: record.work.proof_plan,
        closure_evidence: record.work.closure_evidence,
        blocked_by: record.work.blocked_by,
        blocks: record.work.blocks,
        target: record.work.target,
        implementation_plan: record.work.implementation_plan,
        release_contract_json: record
            .work
            .release_contract
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .unwrap_or(None),
        belief_harvest_decision: record.work.belief_harvest_decision,
        body_path: record.body_path,
        narrative_summary,
        path: record.path,
    }
}

pub(crate) fn slate_work_event(
    value: serde_json::Value,
) -> exports::patina::slate::control::WorkEvent {
    exports::patina::slate::control::WorkEvent {
        work_id: value
            .get("work_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        event_type: value
            .get("event_type")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        payload_json: value
            .get("payload")
            .map(|v| v.to_string())
            .unwrap_or_else(|| "{}".to_string()),
        created_at: value
            .get("created_at")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
    }
}

pub(crate) fn normalize_work_items(items: &[String]) -> Vec<(String, String, bool)> {
    items
        .iter()
        .map(|text| {
            let checked = text.contains("[x]") || text.contains("checked: true");
            (slugify(text), text.clone(), checked)
        })
        .collect()
}

pub(crate) fn work_progress(work: &SlateWorkFile) -> (usize, usize, Vec<String>, Vec<String>) {
    let criteria = normalize_work_items(&work.proof_plan);
    let total = criteria.len();
    let checked = criteria.iter().filter(|(_, _, checked)| *checked).count();
    let completed = criteria
        .iter()
        .filter(|(_, _, checked)| *checked)
        .map(|(_, text, _)| text.clone())
        .collect();
    let open = criteria
        .iter()
        .filter(|(_, _, checked)| !*checked)
        .map(|(_, text, _)| text.clone())
        .collect();
    (checked, total, completed, open)
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct WorkStateEventView {
    pub(crate) work_id: String,
    pub(crate) event_type: String,
    pub(crate) payload_json: String,
    pub(crate) created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct WorkProgressView {
    pub(crate) checked: usize,
    pub(crate) total: usize,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct WorkStateView {
    pub(crate) work_id: String,
    pub(crate) status: String,
    pub(crate) title: String,
    pub(crate) human_request: String,
    pub(crate) user_alignment: String,
    pub(crate) user_value: String,
    pub(crate) scope: Vec<String>,
    pub(crate) non_goals: Vec<String>,
    pub(crate) stop_condition: String,
    pub(crate) narrative_summary: String,
    pub(crate) progress: WorkProgressView,
    pub(crate) completed_items: Vec<String>,
    pub(crate) open_items: Vec<String>,
    pub(crate) closure_evidence: Vec<String>,
    pub(crate) unresolved_blockers: Vec<String>,
    pub(crate) resolved_blockers: Vec<String>,
    pub(crate) dependency_warnings: Vec<String>,
    pub(crate) cleanup_candidates: Vec<String>,
    pub(crate) next_safe_action: String,
    pub(crate) recent_events: Vec<WorkStateEventView>,
    pub(crate) evidence: Vec<String>,
    pub(crate) path: String,
    pub(crate) body_path: Option<String>,
}

fn work_state_event(value: serde_json::Value) -> WorkStateEventView {
    WorkStateEventView {
        work_id: value
            .get("work_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        event_type: value
            .get("event_type")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        payload_json: value
            .get("payload")
            .map(|v| v.to_string())
            .unwrap_or_else(|| "{}".to_string()),
        created_at: value
            .get("created_at")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
    }
}

fn cleanup_candidates(work: &SlateWorkFile, checked: usize, total: usize) -> Vec<String> {
    let mut candidates = Vec::new();
    if work.human_request.trim().is_empty() {
        candidates.push("missing human_request".to_string());
    }
    if work.user_alignment.trim().is_empty() {
        candidates.push("missing user_alignment".to_string());
    }
    if work.user_value.trim().is_empty() {
        candidates.push("missing user_value".to_string());
    }
    if work.scope.is_empty() {
        candidates.push("missing scope".to_string());
    }
    if work.non_goals.is_empty() {
        candidates.push("missing non_goals".to_string());
    }
    if work.stop_condition.trim().is_empty() {
        candidates.push("missing stop_condition".to_string());
    }
    if total == 0 {
        candidates.push("missing proof_plan".to_string());
    }
    if checked > 0 && work.closure_evidence.is_empty() {
        candidates.push("proof started without closure_evidence".to_string());
    }
    candidates
}

fn next_safe_action(
    work: &SlateWorkFile,
    checked: usize,
    total: usize,
    blockers: &[String],
) -> String {
    if !blockers.is_empty() {
        return format!(
            "Resolve blockers before continuing: {}",
            blockers.join(", ")
        );
    }
    if work.status == "draft" {
        return "Fill required intent, scope, stop condition, and proof fields, then promote to ready.".to_string();
    }
    if work.status == "ready" {
        return "Activate this work item before implementation.".to_string();
    }
    if total == 0 {
        return "Add a proof plan before implementation continues.".to_string();
    }
    if checked < total {
        return "Continue implementation and check the remaining proof items.".to_string();
    }
    if work.closure_evidence.is_empty() {
        return "Add closure evidence before completing this work item.".to_string();
    }
    if work.belief_harvest_decision.is_none() {
        return "Record the belief harvest decision before completion.".to_string();
    }
    if work.status.is_terminal() {
        return "Work is terminal; archive when recovery window is no longer needed.".to_string();
    }
    "Complete this work item.".to_string()
}

pub(crate) fn work_state_view(
    root: &Path,
    records: &[SlateWorkRecord],
    record: &SlateWorkRecord,
) -> Result<WorkStateView, String> {
    let status_map = slate_status_map(records);
    let blockers = unresolved_blockers(&record.work, &status_map);
    let resolved = resolved_blockers(&record.work, &status_map);
    let warnings = dependency_warnings(&record.work, &status_map);
    let (checked, total, completed_items, open_items) = work_progress(&record.work);
    let mut recent_events = load_slate_events(root, &record.work.id)?
        .into_iter()
        .map(work_state_event)
        .collect::<Vec<_>>();
    recent_events.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    recent_events.truncate(10);

    let mut evidence = vec![record.path.clone(), "layer/slate/events.jsonl".to_string()];
    if let Some(body_path) = &record.body_path {
        evidence.push(body_path.clone());
    }
    evidence.extend(record.work.closure_evidence.iter().cloned());
    evidence.sort();
    evidence.dedup();

    Ok(WorkStateView {
        work_id: record.work.id.clone(),
        status: record.work.status.to_string(),
        title: record.work.title.clone(),
        human_request: record.work.human_request.clone(),
        user_alignment: record.work.user_alignment.clone(),
        user_value: record.work.user_value.clone(),
        scope: record.work.scope.clone(),
        non_goals: record.work.non_goals.clone(),
        stop_condition: record.work.stop_condition.clone(),
        narrative_summary: work_narrative_summary(record),
        progress: WorkProgressView { checked, total },
        completed_items,
        open_items,
        closure_evidence: record.work.closure_evidence.clone(),
        unresolved_blockers: blockers.clone(),
        resolved_blockers: resolved,
        dependency_warnings: warnings,
        cleanup_candidates: cleanup_candidates(&record.work, checked, total),
        next_safe_action: next_safe_action(&record.work, checked, total, &blockers),
        recent_events,
        evidence,
        path: record.path.clone(),
        body_path: record.body_path.clone(),
    })
}

pub(crate) fn work_state_json(
    root: &Path,
    records: &[SlateWorkRecord],
    record: &SlateWorkRecord,
) -> Result<serde_json::Value, String> {
    serde_json::to_value(work_state_view(root, records, record)?).map_err(|e| e.to_string())
}

pub(crate) fn work_state_result(
    root: &Path,
    records: &[SlateWorkRecord],
    record: SlateWorkRecord,
) -> Result<exports::patina::slate::control::WorkStateResult, String> {
    let state = work_state_view(root, records, &record)?;
    Ok(exports::patina::slate::control::WorkStateResult {
        work_id: state.work_id,
        status: state.status,
        title: state.title,
        human_request: state.human_request,
        user_alignment: state.user_alignment,
        user_value: state.user_value,
        scope: state.scope,
        non_goals: state.non_goals,
        stop_condition: state.stop_condition,
        narrative_summary: state.narrative_summary,
        progress: exports::patina::slate::control::ProgressSummary {
            checked: u32::try_from(state.progress.checked)
                .map_err(|_| "checked exceeds u32".to_string())?,
            total: u32::try_from(state.progress.total)
                .map_err(|_| "total exceeds u32".to_string())?,
        },
        completed_items: state.completed_items,
        open_items: state.open_items,
        closure_evidence: state.closure_evidence,
        unresolved_blockers: state.unresolved_blockers,
        resolved_blockers: state.resolved_blockers,
        dependency_warnings: state.dependency_warnings,
        cleanup_candidates: state.cleanup_candidates,
        next_safe_action: state.next_safe_action,
        recent_events: state
            .recent_events
            .into_iter()
            .map(|event| exports::patina::slate::control::WorkEvent {
                work_id: event.work_id,
                event_type: event.event_type,
                payload_json: event.payload_json,
                created_at: event.created_at,
            })
            .collect(),
        evidence: state.evidence,
        path: state.path,
        body_path: state.body_path,
    })
}

pub(crate) fn closure_gates(work: &SlateWorkFile) -> Vec<String> {
    let allium_ok = !work.allium_anchors.is_empty()
        || (work.kind == "refactor"
            && work
                .user_alignment
                .to_ascii_lowercase()
                .contains("no behavior"));
    vec![
        format!("Allium intent aligned: {}", allium_ok),
        format!("Proof plan present: {}", !work.proof_plan.is_empty()),
        format!(
            "Closure evidence present: {}",
            !work.closure_evidence.is_empty()
        ),
        format!(
            "Belief harvest decision present: {}",
            work.belief_harvest_decision.is_some()
        ),
    ]
}

pub(crate) fn validate_complete_gate(work: &SlateWorkFile) -> Result<(), String> {
    let (checked, total, _, _) = work_progress(work);
    if total == 0 || checked != total {
        return Err(format!(
            "Slate work '{}' cannot complete: proof plan is not fully checked",
            work.id
        ));
    }
    if work.closure_evidence.is_empty() {
        return Err(format!(
            "Slate work '{}' cannot complete: missing closure evidence",
            work.id
        ));
    }
    if work.belief_harvest_decision.is_none() {
        return Err(format!(
            "Slate work '{}' cannot complete: missing belief harvest decision",
            work.id
        ));
    }
    Ok(())
}

pub(crate) fn work_prompt_result(
    root: &Path,
    records: &[SlateWorkRecord],
    record: SlateWorkRecord,
) -> exports::patina::slate::control::WorkPromptResult {
    let status_map = slate_status_map(records);
    let closure_gates = closure_gates(&record.work);
    let narrative_summary = work_narrative_summary(&record);
    let narrative_context = work_narrative_context(root, &record);
    let read_first = work_read_first(root, &record);
    let direction = dependency_direction_summary(&record.work, &status_map);
    exports::patina::slate::control::WorkPromptResult {
        work_id: record.work.id,
        status: record.work.status.to_string(),
        title: record.work.title,
        human_request: record.work.human_request,
        narrative_summary,
        narrative_context,
        read_first,
        allium_anchors: record.work.allium_anchors,
        implementation_plan: record.work.implementation_plan,
        proof_plan: record.work.proof_plan,
        belief_refs: record.work.belief_refs,
        blocked_by: record.work.blocked_by,
        blocks: record.work.blocks,
        direction,
        closure_gates,
        path: record.path,
    }
}

pub(crate) fn work_handoff_result(
    root: &Path,
    records: &[SlateWorkRecord],
    record: SlateWorkRecord,
) -> Result<exports::patina::slate::control::WorkHandoffResult, String> {
    let status_map = slate_status_map(records);
    let (checked, total, completed_items, open_items) = work_progress(&record.work);
    let narrative_summary = work_narrative_summary(&record);
    let narrative_context = work_narrative_context(root, &record);
    let dependency_warnings = dependency_warnings(&record.work, &status_map);
    let direction = dependency_direction_summary(&record.work, &status_map);
    Ok(exports::patina::slate::control::WorkHandoffResult {
        work_id: record.work.id,
        status: record.work.status.to_string(),
        title: record.work.title,
        narrative_summary,
        narrative_context,
        progress: exports::patina::slate::control::ProgressSummary {
            checked: u32::try_from(checked).map_err(|_| "checked exceeds u32".to_string())?,
            total: u32::try_from(total).map_err(|_| "total exceeds u32".to_string())?,
        },
        completed_items,
        open_items,
        next_steps: record.work.implementation_plan,
        closure_evidence: record.work.closure_evidence,
        belief_harvest_decision: record.work.belief_harvest_decision,
        blocked_by: record.work.blocked_by,
        blocks: record.work.blocks,
        direction,
        dependency_warnings,
        path: record.path,
    })
}
