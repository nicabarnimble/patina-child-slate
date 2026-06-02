use crate::model::{SlateWorkFile, SlateWorkRecord, WorkStatus};
use crate::store::{append_slate_event, find_slate_work, load_slate_work, write_slate_work_file};
use std::path::Path;

fn ready_gate_failures(work: &SlateWorkFile) -> Vec<String> {
    let mut failures = Vec::new();
    if work.human_request.trim().is_empty() {
        failures.push(
            "human_request is empty; set `human_request:set` to the user's request".to_string(),
        );
    }
    if work.user_alignment.trim().is_empty() {
        failures.push(
            "user_alignment is empty; set `user_alignment:set` to why this matches the request"
                .to_string(),
        );
    }
    if work.proof_plan.is_empty() {
        failures
            .push("proof_plan is empty; add a checkable item with `proof_plan:add`".to_string());
    }

    let allium_ok = !work.allium_anchors.is_empty()
        || (work.kind == "refactor"
            && work
                .user_alignment
                .to_ascii_lowercase()
                .contains("no behavior"));
    if !allium_ok {
        failures.push("Allium/allium_anchors is empty; add an anchor with `allium_anchors:add`, or for a refactor include a no-behavior-change rationale in user_alignment".to_string());
    }

    failures
}

pub(crate) fn validate_ready_gate(work: &SlateWorkFile) -> Result<(), String> {
    let failures = ready_gate_failures(work);
    if !failures.is_empty() {
        return Err(format!(
            "Slate work '{}' is not ready. Missing gates:\n- {}",
            work.id,
            failures.join("\n- ")
        ));
    }
    Ok(())
}

fn transition_slate_work(
    root: &Path,
    id: &str,
    event_type: &str,
    force: bool,
    transition: impl FnOnce(&mut SlateWorkFile) -> Result<WorkStatus, String>,
) -> Result<SlateWorkRecord, String> {
    let mut records = load_slate_work(root)?;
    let mut record = find_slate_work(&records, id)?.clone();
    let from = record.work.status;
    let to = transition(&mut record.work)?;
    let saved = write_slate_work_file(root, &mut record.work)?;
    append_slate_event(
        root,
        &saved.work.id,
        event_type,
        serde_json::json!({"from": from, "to": to, "force": force}),
    )?;
    records.clear();
    Ok(saved)
}

pub(crate) fn promote_slate_work(
    root: &Path,
    id: &str,
    force: bool,
) -> Result<SlateWorkRecord, String> {
    transition_slate_work(root, id, "promoted", force, |work| {
        let to = match work.status {
            WorkStatus::Draft => {
                if !force {
                    validate_ready_gate(work)?;
                }
                WorkStatus::Ready
            }
            WorkStatus::Ready => WorkStatus::Active,
            other => {
                return Err(format!(
                    "cannot promote Slate work '{}' from status '{}'. Valid promotions: draft -> ready, ready -> active. Use activate-work for a single explicit activation path.",
                    work.id, other
                ))
            }
        };
        work.status = to;
        Ok(to)
    })
}

pub(crate) fn activate_slate_work(
    root: &Path,
    id: &str,
    force: bool,
) -> Result<SlateWorkRecord, String> {
    transition_slate_work(root, id, "activated", force, |work| {
        match work.status {
            WorkStatus::Draft | WorkStatus::Ready => {
                if !force && work.status == WorkStatus::Draft {
                    validate_ready_gate(work)?;
                }
                work.status = WorkStatus::Active;
                Ok(WorkStatus::Active)
            }
            WorkStatus::Active => Err(format!(
                "Slate work '{}' is already active; no activation needed",
                work.id
            )),
            other => Err(format!(
                "cannot activate Slate work '{}' from status '{}'. Valid activation statuses: draft or ready",
                work.id, other
            )),
        }
    })
}
