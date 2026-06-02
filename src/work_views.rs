use crate::dependency_graph::{dependency_warnings, slate_status_map};
use crate::exports;
use crate::model::{SlateWorkFile, SlateWorkRecord};
use crate::narrative::{
    dependency_direction_summary, work_narrative_context, work_narrative_summary, work_read_first,
};
use crate::text::slugify;
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
