use crate::dependency_graph::{
    reconcile_slate_dependencies, slate_status_map, unresolved_blockers,
};
use crate::dispatch::dispatch_data_from_envelope;
use crate::exports;
use crate::lifecycle::{activate_slate_work, promote_slate_work};
use crate::model::{default_slate_status, normalize_slate_kind, SlateWorkFile, WorkStatus};
use crate::narrative::effective_work_body;
use crate::patina::git::git;
use crate::runtime::{resolve_project_root_from_hint, to_repo_relative, with_project_root_cwd};
use crate::slate_body::{update_slate_work_body, write_slate_work_body};
use crate::spec_bridge::{
    build_handoff_packet, build_prompt_packet, find_spec, handle_archive, handle_check,
    handle_complete, handle_next, handle_show, load_specs,
};
use crate::store::{
    append_slate_event, create_slate_work_file, find_slate_work, load_slate_events,
    load_slate_work, slate_work_path, timestamp, update_slate_work, validate_slate_id,
    write_slate_work_file, SLATE_EVENTS_PATH,
};
use crate::telemetry;
use crate::text::extract_title;
use crate::work_fields::{
    apply_list_field, apply_optional_string_field, apply_release_contract_field,
    apply_required_string_field, apply_status_field, parse_set_work_field_spec,
    unsupported_set_work_field_error,
};
use crate::work_views::{
    normalize_work_items, slate_work_event, slate_work_record, slate_work_summary,
    validate_complete_gate, work_handoff_result, work_prompt_result, work_state_result,
};
use crate::SlateManager;
use patina_sdk::toys;
use std::fs;

impl exports::patina::slate::control::Guest for SlateManager {
    fn list_work(
        req: exports::patina::slate::control::WorkListRequest,
    ) -> Result<Vec<exports::patina::slate::control::WorkSummary>, String> {
        let project_root = resolve_project_root_from_hint(req.project.as_deref())?;
        with_project_root_cwd(&project_root, || {
            let records = load_slate_work(&project_root)?;
            Ok(records
                .into_iter()
                .filter(|record| {
                    let status_ok = req
                        .status
                        .as_deref()
                        .is_none_or(|expected| record.work.status == expected);
                    let kind_ok = req
                        .kind
                        .as_deref()
                        .is_none_or(|expected| record.work.kind == normalize_slate_kind(expected));
                    status_ok && kind_ok
                })
                .map(slate_work_summary)
                .collect())
        })
    }

    fn ready_work(
        req: exports::patina::slate::control::WorkListRequest,
    ) -> Result<Vec<exports::patina::slate::control::WorkSummary>, String> {
        let project_root = resolve_project_root_from_hint(req.project.as_deref())?;
        with_project_root_cwd(&project_root, || {
            let records = load_slate_work(&project_root)?;
            let status_map = slate_status_map(&records);
            Ok(records
                .into_iter()
                .filter(|record| {
                    (record.work.status == "ready" || record.work.status == "active")
                        && unresolved_blockers(&record.work, &status_map).is_empty()
                })
                .filter(|record| {
                    req.kind
                        .as_deref()
                        .is_none_or(|kind| record.work.kind == normalize_slate_kind(kind))
                })
                .map(slate_work_summary)
                .collect())
        })
    }

    fn blocked_work(
        req: exports::patina::slate::control::WorkListRequest,
    ) -> Result<Vec<exports::patina::slate::control::WorkSummary>, String> {
        let project_root = resolve_project_root_from_hint(req.project.as_deref())?;
        with_project_root_cwd(&project_root, || {
            let records = load_slate_work(&project_root)?;
            let status_map = slate_status_map(&records);
            Ok(records
                .into_iter()
                .filter(|record| {
                    record.work.status == "blocked"
                        || !unresolved_blockers(&record.work, &status_map).is_empty()
                })
                .filter(|record| {
                    req.kind
                        .as_deref()
                        .is_none_or(|kind| record.work.kind == normalize_slate_kind(kind))
                })
                .map(slate_work_summary)
                .collect())
        })
    }

    fn next_work(
        req: exports::patina::slate::control::WorkListRequest,
    ) -> Result<Vec<exports::patina::slate::control::WorkRecommendation>, String> {
        let project_root = resolve_project_root_from_hint(req.project.as_deref())?;
        with_project_root_cwd(&project_root, || {
            let records = load_slate_work(&project_root)?;
            let status_map = slate_status_map(&records);
            let mut rows = records
                .into_iter()
                .filter(|record| {
                    req.kind
                        .as_deref()
                        .is_none_or(|kind| record.work.kind == normalize_slate_kind(kind))
                })
                .filter_map(|record| {
                    let unresolved = unresolved_blockers(&record.work, &status_map);
                    if !unresolved.is_empty() {
                        return None;
                    }
                    let (priority, reason) = match record.work.status.as_str() {
                        "active" => (1, "Currently active".to_string()),
                        "blocked"
                            if unresolved.is_empty() && !record.work.blocked_by.is_empty() =>
                        {
                            (
                                2,
                                "Blockers complete — stale blocked status; resume or reconcile"
                                    .to_string(),
                            )
                        }
                        "ready" => (3, "Ready to start".to_string()),
                        "blocked" if record.work.blocked_by.is_empty() => {
                            (4, "Blocked without dependency".to_string())
                        }
                        "draft" => (5, "Draft needs intent/proof alignment".to_string()),
                        _ => return None,
                    };
                    Some(exports::patina::slate::control::WorkRecommendation {
                        id: record.work.id,
                        status: record.work.status.to_string(),
                        reason,
                        priority,
                        path: record.path,
                    })
                })
                .collect::<Vec<_>>();
            rows.sort_by(|a, b| a.priority.cmp(&b.priority).then(a.id.cmp(&b.id)));
            Ok(rows)
        })
    }

    fn show_work(
        req: exports::patina::slate::control::WorkIdRequest,
    ) -> Result<exports::patina::slate::control::WorkRecord, String> {
        let project_root = resolve_project_root_from_hint(req.project.as_deref())?;
        with_project_root_cwd(&project_root, || {
            let records = load_slate_work(&project_root)?;
            Ok(slate_work_record(
                &project_root,
                find_slate_work(&records, &req.id)?.clone(),
            ))
        })
    }

    fn history_work(
        req: exports::patina::slate::control::WorkIdRequest,
    ) -> Result<Vec<exports::patina::slate::control::WorkEvent>, String> {
        let project_root = resolve_project_root_from_hint(req.project.as_deref())?;
        with_project_root_cwd(&project_root, || {
            let events = load_slate_events(&project_root, &req.id)?;
            Ok(events.into_iter().map(slate_work_event).collect())
        })
    }

    fn create_work(
        req: exports::patina::slate::control::CreateWorkRequest,
    ) -> Result<exports::patina::slate::control::WorkRecord, String> {
        let project_root = resolve_project_root_from_hint(req.project.as_deref())?;
        with_project_root_cwd(&project_root, || {
            let mut work = SlateWorkFile {
                id: req.id,
                title: req.title,
                kind: normalize_slate_kind(&req.kind),
                status: default_slate_status(),
                human_request: req.human_request,
                user_value: String::new(),
                scope: Vec::new(),
                non_goals: Vec::new(),
                stop_condition: String::new(),
                allium_anchors: req.allium_anchors,
                user_alignment: req.user_alignment,
                belief_refs: Vec::new(),
                proof_plan: Vec::new(),
                closure_evidence: Vec::new(),
                blocked_by: Vec::new(),
                blocks: Vec::new(),
                target: None,
                implementation_plan: Vec::new(),
                release_contract: None,
                belief_harvest_decision: None,
                created_at: None,
                updated_at: None,
                closed_at: None,
                block_reason: None,
                pause_reason: None,
            };
            create_slate_work_file(&project_root, &mut work)
                .map(|record| slate_work_record(&project_root, record))
        })
    }

    fn set_work(
        req: exports::patina::slate::control::SetWorkRequest,
    ) -> Result<exports::patina::slate::control::WorkRecord, String> {
        let project_root = resolve_project_root_from_hint(req.project.as_deref())?;
        with_project_root_cwd(&project_root, || {
            let spec = parse_set_work_field_spec(&req.field)?;
            if spec.field == "body" {
                return update_slate_work_body(&project_root, &req.id, &spec.operation, &req.value)
                    .map(|record| slate_work_record(&project_root, record));
            }

            let field = req.field.clone();
            let value = req.value.clone();
            let dependency_field = matches!(spec.field, "blocked_by" | "blocks");
            let spec_for_update = spec.clone();
            let saved = update_slate_work(
                &project_root,
                &req.id,
                "set",
                serde_json::json!({"field": field, "value": value}),
                |work| match spec_for_update.field {
                    "title" => apply_required_string_field(
                        "title",
                        &mut work.title,
                        &spec_for_update.operation,
                        req.value.clone(),
                    ),
                    "status" => apply_status_field(
                        &mut work.status,
                        &spec_for_update.operation,
                        req.value.clone(),
                    ),
                    "human_request" => apply_required_string_field(
                        "human_request",
                        &mut work.human_request,
                        &spec_for_update.operation,
                        req.value.clone(),
                    ),
                    "user_value" => apply_required_string_field(
                        "user_value",
                        &mut work.user_value,
                        &spec_for_update.operation,
                        req.value.clone(),
                    ),
                    "scope" => apply_list_field(
                        "scope",
                        &mut work.scope,
                        &spec_for_update.operation,
                        req.value.clone(),
                    ),
                    "non_goals" => apply_list_field(
                        "non_goals",
                        &mut work.non_goals,
                        &spec_for_update.operation,
                        req.value.clone(),
                    ),
                    "stop_condition" => apply_required_string_field(
                        "stop_condition",
                        &mut work.stop_condition,
                        &spec_for_update.operation,
                        req.value.clone(),
                    ),
                    "target" => apply_optional_string_field(
                        "target",
                        &mut work.target,
                        &spec_for_update.operation,
                        req.value.clone(),
                    ),
                    "user_alignment" => apply_required_string_field(
                        "user_alignment",
                        &mut work.user_alignment,
                        &spec_for_update.operation,
                        req.value.clone(),
                    ),
                    "belief_harvest_decision" => apply_optional_string_field(
                        "belief_harvest_decision",
                        &mut work.belief_harvest_decision,
                        &spec_for_update.operation,
                        req.value.clone(),
                    ),
                    "proof_plan" => apply_list_field(
                        "proof_plan",
                        &mut work.proof_plan,
                        &spec_for_update.operation,
                        req.value.clone(),
                    ),
                    "implementation_plan" => apply_list_field(
                        "implementation_plan",
                        &mut work.implementation_plan,
                        &spec_for_update.operation,
                        req.value.clone(),
                    ),
                    "closure_evidence" => apply_list_field(
                        "closure_evidence",
                        &mut work.closure_evidence,
                        &spec_for_update.operation,
                        req.value.clone(),
                    ),
                    "blocked_by" => apply_list_field(
                        "blocked_by",
                        &mut work.blocked_by,
                        &spec_for_update.operation,
                        req.value.clone(),
                    ),
                    "blocks" => apply_list_field(
                        "blocks",
                        &mut work.blocks,
                        &spec_for_update.operation,
                        req.value.clone(),
                    ),
                    "release_contract" => apply_release_contract_field(
                        &mut work.release_contract,
                        &spec_for_update.operation,
                        req.value.clone(),
                    ),
                    "allium_anchor" => apply_list_field(
                        "allium_anchors",
                        &mut work.allium_anchors,
                        &spec_for_update.operation,
                        req.value.clone(),
                    ),
                    "belief_ref" => apply_list_field(
                        "belief_refs",
                        &mut work.belief_refs,
                        &spec_for_update.operation,
                        req.value.clone(),
                    ),
                    _ => Err(unsupported_set_work_field_error(&req.field)),
                },
            )?;

            if dependency_field {
                reconcile_slate_dependencies(&project_root, "set-work")?;
                let records = load_slate_work(&project_root)?;
                return Ok(slate_work_record(
                    &project_root,
                    find_slate_work(&records, &req.id)?.clone(),
                ));
            }

            Ok(slate_work_record(&project_root, saved))
        })
    }

    fn promote_work(
        req: exports::patina::slate::control::WorkStatusRequest,
    ) -> Result<exports::patina::slate::control::WorkRecord, String> {
        let project_root = resolve_project_root_from_hint(req.project.as_deref())?;
        with_project_root_cwd(&project_root, || {
            promote_slate_work(&project_root, &req.id, req.force)
                .map(|record| slate_work_record(&project_root, record))
        })
    }

    fn activate_work(
        req: exports::patina::slate::control::WorkStatusRequest,
    ) -> Result<exports::patina::slate::control::WorkRecord, String> {
        let project_root = resolve_project_root_from_hint(req.project.as_deref())?;
        with_project_root_cwd(&project_root, || {
            activate_slate_work(&project_root, &req.id, req.force)
                .map(|record| slate_work_record(&project_root, record))
        })
    }

    fn check_work(
        req: exports::patina::slate::control::WorkIdRequest,
    ) -> Result<exports::patina::slate::control::WorkCheckResult, String> {
        let project_root = resolve_project_root_from_hint(req.project.as_deref())?;
        with_project_root_cwd(&project_root, || {
            let records = load_slate_work(&project_root)?;
            let record = find_slate_work(&records, &req.id)?;
            let criteria = normalize_work_items(&record.work.proof_plan);
            let total = criteria.len();
            let checked = criteria.iter().filter(|(_, _, checked)| *checked).count();
            let unchecked = criteria
                .into_iter()
                .filter(|(_, _, checked)| !*checked)
                .map(
                    |(id, text, _)| exports::patina::slate::control::UncheckedCriterion {
                        id,
                        text,
                    },
                )
                .collect::<Vec<_>>();
            let passed = checked == total && total > 0;
            telemetry::record_check(total, checked, passed)?;
            Ok(exports::patina::slate::control::WorkCheckResult {
                work_id: req.id,
                total: u32::try_from(total).map_err(|_| "total exceeds u32".to_string())?,
                checked: u32::try_from(checked).map_err(|_| "checked exceeds u32".to_string())?,
                unchecked,
                passed,
            })
        })
    }

    fn pause_work(
        req: exports::patina::slate::control::PauseWorkRequest,
    ) -> Result<exports::patina::slate::control::WorkRecord, String> {
        let project_root = resolve_project_root_from_hint(req.project.as_deref())?;
        with_project_root_cwd(&project_root, || {
            update_slate_work(
                &project_root,
                &req.id,
                "paused",
                serde_json::json!({"reason": req.reason}),
                |work| {
                    work.status = WorkStatus::Paused;
                    work.pause_reason = Some(req.reason);
                    Ok(())
                },
            )
            .map(|record| slate_work_record(&project_root, record))
        })
    }

    fn resume_work(
        req: exports::patina::slate::control::WorkStatusRequest,
    ) -> Result<exports::patina::slate::control::WorkRecord, String> {
        let project_root = resolve_project_root_from_hint(req.project.as_deref())?;
        with_project_root_cwd(&project_root, || {
            update_slate_work(
                &project_root,
                &req.id,
                "resumed",
                serde_json::json!({"force": req.force}),
                |work| {
                    if !matches!(work.status.as_str(), "paused" | "blocked") && !req.force {
                        return Err(format!(
                            "cannot resume Slate work '{}' from status '{}'",
                            work.id, work.status
                        ));
                    }
                    work.status = WorkStatus::Ready;
                    work.pause_reason = None;
                    Ok(())
                },
            )
            .map(|record| slate_work_record(&project_root, record))
        })
    }

    fn block_work(
        req: exports::patina::slate::control::BlockWorkRequest,
    ) -> Result<exports::patina::slate::control::WorkRecord, String> {
        let project_root = resolve_project_root_from_hint(req.project.as_deref())?;
        with_project_root_cwd(&project_root, || {
            update_slate_work(
                &project_root,
                &req.id,
                "blocked",
                serde_json::json!({"reason": req.reason, "blocked_by": req.blocked_by}),
                |work| {
                    work.status = WorkStatus::Blocked;
                    work.block_reason = Some(req.reason);
                    if let Some(blocker) = req.blocked_by {
                        if !work.blocked_by.contains(&blocker) {
                            work.blocked_by.push(blocker);
                        }
                    }
                    Ok(())
                },
            )?;
            reconcile_slate_dependencies(&project_root, "block-work")?;
            let records = load_slate_work(&project_root)?;
            Ok(slate_work_record(
                &project_root,
                find_slate_work(&records, &req.id)?.clone(),
            ))
        })
    }

    fn abandon_work(
        req: exports::patina::slate::control::PauseWorkRequest,
    ) -> Result<exports::patina::slate::control::WorkRecord, String> {
        let project_root = resolve_project_root_from_hint(req.project.as_deref())?;
        with_project_root_cwd(&project_root, || {
            update_slate_work(
                &project_root,
                &req.id,
                "abandoned",
                serde_json::json!({"reason": req.reason}),
                |work| {
                    work.status = WorkStatus::Abandoned;
                    work.closed_at = Some(timestamp());
                    work.block_reason = Some(req.reason);
                    Ok(())
                },
            )?;
            reconcile_slate_dependencies(&project_root, "abandon-work")?;
            let records = load_slate_work(&project_root)?;
            Ok(slate_work_record(
                &project_root,
                find_slate_work(&records, &req.id)?.clone(),
            ))
        })
    }

    fn rename_work(
        req: exports::patina::slate::control::RenameWorkRequest,
    ) -> Result<exports::patina::slate::control::WorkRecord, String> {
        let project_root = resolve_project_root_from_hint(req.project.as_deref())?;
        with_project_root_cwd(&project_root, || {
            validate_slate_id(&req.new_id)?;
            let records = load_slate_work(&project_root)?;
            let mut record = find_slate_work(&records, &req.id)?.clone();
            let old_dir = slate_work_path(&project_root, &req.id)
                .parent()
                .unwrap()
                .to_path_buf();
            let new_path = slate_work_path(&project_root, &req.new_id);
            if new_path.exists() {
                return Err(format!("Slate work '{}' already exists", req.new_id));
            }
            let body = effective_work_body(&record);
            record.work.id = req.new_id.clone();
            let saved = write_slate_work_file(&project_root, &mut record.work)?;
            if !body.trim().is_empty() {
                write_slate_work_body(&project_root, &saved.work.id, &body)?;
            }
            if old_dir.exists() {
                fs::remove_dir_all(&old_dir)
                    .map_err(|e| format!("remove {}: {}", old_dir.display(), e))?;
            }
            append_slate_event(
                &project_root,
                &saved.work.id,
                "renamed",
                serde_json::json!({"old_id": req.id}),
            )?;
            let records = load_slate_work(&project_root)?;
            Ok(slate_work_record(
                &project_root,
                find_slate_work(&records, &saved.work.id)?.clone(),
            ))
        })
    }

    fn reopen_work(
        req: exports::patina::slate::control::WorkStatusRequest,
    ) -> Result<exports::patina::slate::control::WorkRecord, String> {
        let project_root = resolve_project_root_from_hint(req.project.as_deref())?;
        with_project_root_cwd(&project_root, || {
            update_slate_work(
                &project_root,
                &req.id,
                "reopened",
                serde_json::json!({"force": req.force}),
                |work| {
                    if !matches!(work.status.as_str(), "complete" | "abandoned") && !req.force {
                        return Err(format!(
                            "cannot reopen Slate work '{}' from status '{}'",
                            work.id, work.status
                        ));
                    }
                    work.status = WorkStatus::Active;
                    work.closed_at = None;
                    Ok(())
                },
            )
            .map(|record| slate_work_record(&project_root, record))
        })
    }

    fn split_work(
        req: exports::patina::slate::control::SplitWorkRequest,
    ) -> Result<exports::patina::slate::control::WorkRecord, String> {
        let project_root = resolve_project_root_from_hint(req.project.as_deref())?;
        with_project_root_cwd(&project_root, || {
            let records = load_slate_work(&project_root)?;
            let parent = find_slate_work(&records, &req.id)?;
            let mut child = parent.work.clone();
            child.id = req.new_id;
            child.title = req.title;
            child.status = WorkStatus::Draft;
            child.closure_evidence.clear();
            child.closed_at = None;
            child.created_at = None;
            child.updated_at = None;
            let saved = create_slate_work_file(&project_root, &mut child)?;
            let child_body = format!(
                "{}\n\n## Split From\n- Parent: {}\n- Split title: {}\n",
                effective_work_body(parent),
                parent.work.id,
                saved.work.title
            );
            write_slate_work_body(&project_root, &saved.work.id, &child_body)?;
            append_slate_event(
                &project_root,
                &parent.work.id,
                "split",
                serde_json::json!({"child_id": saved.work.id}),
            )?;
            let records = load_slate_work(&project_root)?;
            Ok(slate_work_record(
                &project_root,
                find_slate_work(&records, &saved.work.id)?.clone(),
            ))
        })
    }

    fn prompt_work(
        req: exports::patina::slate::control::WorkIdRequest,
    ) -> Result<exports::patina::slate::control::WorkPromptResult, String> {
        let project_root = resolve_project_root_from_hint(req.project.as_deref())?;
        with_project_root_cwd(&project_root, || {
            let records = load_slate_work(&project_root)?;
            Ok(work_prompt_result(
                &project_root,
                &records,
                find_slate_work(&records, &req.id)?.clone(),
            ))
        })
    }

    fn handoff_work(
        req: exports::patina::slate::control::WorkIdRequest,
    ) -> Result<exports::patina::slate::control::WorkHandoffResult, String> {
        let project_root = resolve_project_root_from_hint(req.project.as_deref())?;
        with_project_root_cwd(&project_root, || {
            let records = load_slate_work(&project_root)?;
            work_handoff_result(
                &project_root,
                &records,
                find_slate_work(&records, &req.id)?.clone(),
            )
        })
    }

    fn packet_work(
        req: exports::patina::slate::control::WorkIdRequest,
    ) -> Result<exports::patina::slate::control::WorkPacketResult, String> {
        let prompt = Self::prompt_work(exports::patina::slate::control::WorkIdRequest {
            project: req.project.clone(),
            id: req.id.clone(),
        })?;
        let handoff = Self::handoff_work(exports::patina::slate::control::WorkIdRequest {
            project: req.project.clone(),
            id: req.id.clone(),
        })?;
        let project_root = resolve_project_root_from_hint(req.project.as_deref())?;
        let state = with_project_root_cwd(&project_root, || {
            let records = load_slate_work(&project_root)?;
            work_state_result(
                &project_root,
                &records,
                find_slate_work(&records, &req.id)?.clone(),
            )
        })?;
        telemetry::record_packet(
            state.progress.total as usize,
            state.progress.checked as usize,
            state.cleanup_candidates.len(),
        )?;
        Ok(exports::patina::slate::control::WorkPacketResult {
            prompt,
            handoff,
            state,
        })
    }

    fn complete_work(
        req: exports::patina::slate::control::WorkStatusRequest,
    ) -> Result<exports::patina::slate::control::WorkRecord, String> {
        let project_root = resolve_project_root_from_hint(req.project.as_deref())?;
        with_project_root_cwd(&project_root, || {
            update_slate_work(
                &project_root,
                &req.id,
                "completed",
                serde_json::json!({"force": req.force}),
                |work| {
                    if !req.force {
                        validate_complete_gate(work)?;
                    }
                    work.status = WorkStatus::Complete;
                    work.closed_at = Some(timestamp());
                    Ok(())
                },
            )?;
            reconcile_slate_dependencies(&project_root, "complete-work")?;
            let records = load_slate_work(&project_root)?;
            Ok(slate_work_record(
                &project_root,
                find_slate_work(&records, &req.id)?.clone(),
            ))
        })
    }

    fn archive_work(
        req: exports::patina::slate::control::WorkStatusRequest,
    ) -> Result<exports::patina::slate::control::WorkArchiveResult, String> {
        let project_root = resolve_project_root_from_hint(req.project.as_deref())?;
        with_project_root_cwd(&project_root, || {
            let records = load_slate_work(&project_root)?;
            let record = find_slate_work(&records, &req.id)?;
            let status = record.work.status;
            if !matches!(status.as_str(), "complete" | "abandoned") && !req.force {
                return Err(format!(
                    "cannot archive Slate work '{}' from status '{}'",
                    record.work.id, status
                ));
            }

            let tag_name = format!("slate/{}", record.work.id);
            if git::tag_exists(&tag_name)? {
                return Err(format!(
                    "Tag '{}' already exists. Slate work may have been archived previously.",
                    tag_name
                ));
            }

            if !git::is_clean_tracked()? {
                return Err(
                    "Working tree has uncommitted tracked changes. Commit or stash before archiving."
                        .to_string(),
                );
            }

            let work_dir = slate_work_path(&project_root, &record.work.id)
                .parent()
                .ok_or_else(|| format!("Slate work '{}' has no parent directory", record.work.id))?
                .to_path_buf();
            let remove_target = to_repo_relative(&project_root, &work_dir);
            let work_file_rel = record.path.clone();
            let title = record.work.title.clone();

            append_slate_event(
                &project_root,
                &record.work.id,
                "archived",
                serde_json::json!({"force": req.force, "tag": tag_name}),
            )?;
            git::add_paths(std::slice::from_ref(&SLATE_EVENTS_PATH.to_string()))?;
            git::remove_paths(std::slice::from_ref(&remove_target))?;

            let commit_msg = format!(
                "docs: archive {} ({})\n\nSlate work preserved via git tag: {}\nRecover with: git show {}:{}",
                tag_name, status, tag_name, tag_name, work_file_rel
            );
            git::commit(&commit_msg)?;
            git::create_tag_at(&tag_name, "HEAD~1")?;

            toys::log::info(
                "slate-manager",
                &format!(
                    "archived slate id={} status={} target={} title={}",
                    record.work.id, status, remove_target, title
                ),
            );

            Ok(exports::patina::slate::control::WorkArchiveResult {
                work_id: record.work.id.clone(),
                new_status: "archived".to_string(),
                path: remove_target,
                archived: true,
            })
        })
    }

    fn list_specs(
        req: exports::patina::slate::control::ListRequest,
    ) -> Result<Vec<exports::patina::slate::control::SpecSummary>, String> {
        let project_root = resolve_project_root_from_hint(req.project.as_deref())?;
        with_project_root_cwd(&project_root, || {
            let specs = load_specs(&project_root)?;
            let rows = specs
                .into_iter()
                .filter(|spec| {
                    let status_ok = req.status.as_deref().is_none_or(|expected| {
                        spec.frontmatter.status.as_deref() == Some(expected)
                    });
                    let target_ok = req.target.as_deref().is_none_or(|expected| {
                        spec.frontmatter.target.as_deref() == Some(expected)
                    });
                    status_ok && target_ok
                })
                .map(|spec| {
                    let title = extract_title(&spec.body)
                        .or(spec.frontmatter.title.clone())
                        .unwrap_or_else(|| spec.frontmatter.id.clone());
                    exports::patina::slate::control::SpecSummary {
                        id: spec.frontmatter.id,
                        status: spec.frontmatter.status,
                        target: spec.frontmatter.target,
                        title,
                        unscraped: true,
                    }
                })
                .collect::<Vec<_>>();
            Ok(rows)
        })
    }

    fn next_specs(
        req: exports::patina::slate::control::NextRequest,
    ) -> Result<Vec<exports::patina::slate::control::NextRecommendation>, String> {
        let project_root = resolve_project_root_from_hint(req.project.as_deref())?;
        with_project_root_cwd(&project_root, || {
            let value = handle_next(&project_root)?;
            let rows = value
                .as_array()
                .ok_or_else(|| "next result must be an array".to_string())?
                .iter()
                .map(|item| {
                    let obj = item
                        .as_object()
                        .ok_or_else(|| "next item must be an object".to_string())?;
                    let id = obj
                        .get("id")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| "next item missing id".to_string())?
                        .to_string();
                    let status = obj
                        .get("status")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| "next item missing status".to_string())?
                        .to_string();
                    let reason = obj
                        .get("reason")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| "next item missing reason".to_string())?
                        .to_string();
                    let priority = obj
                        .get("priority")
                        .and_then(|v| v.as_u64())
                        .ok_or_else(|| "next item missing priority".to_string())?;
                    let impact = obj.get("impact").and_then(|v| v.as_u64()).unwrap_or(0);
                    let queue_position = obj.get("queue_position").and_then(|v| v.as_u64());
                    Ok(exports::patina::slate::control::NextRecommendation {
                        id,
                        status,
                        reason,
                        priority: u32::try_from(priority)
                            .map_err(|_| "priority exceeds u32".to_string())?,
                        impact: u32::try_from(impact)
                            .map_err(|_| "impact exceeds u32".to_string())?,
                        queue_position: queue_position
                            .map(|value| {
                                u32::try_from(value)
                                    .map_err(|_| "queue_position exceeds u32".to_string())
                            })
                            .transpose()?,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(rows)
        })
    }

    fn check_spec(
        req: exports::patina::slate::control::SpecIdRequest,
    ) -> Result<exports::patina::slate::control::CheckResult, String> {
        let project_root = resolve_project_root_from_hint(req.project.as_deref())?;
        with_project_root_cwd(&project_root, || {
            let value = handle_check(
                &project_root,
                Some(&serde_json::Map::from_iter([(
                    "id".to_string(),
                    serde_json::Value::String(req.id.clone()),
                )])),
            )?;
            let obj = value
                .as_object()
                .ok_or_else(|| "check result must be an object".to_string())?;

            let unchecked = obj
                .get("unchecked")
                .and_then(|v| v.as_array())
                .ok_or_else(|| "check result missing unchecked list".to_string())?
                .iter()
                .map(|item| {
                    let row = item
                        .as_object()
                        .ok_or_else(|| "unchecked item must be an object".to_string())?;
                    Ok(exports::patina::slate::control::UncheckedCriterion {
                        id: row
                            .get("id")
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| "unchecked item missing id".to_string())?
                            .to_string(),
                        text: row
                            .get("text")
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| "unchecked item missing text".to_string())?
                            .to_string(),
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;

            Ok(exports::patina::slate::control::CheckResult {
                spec_id: obj
                    .get("spec_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "check result missing spec_id".to_string())?
                    .to_string(),
                total: u32::try_from(
                    obj.get("total")
                        .and_then(|v| v.as_u64())
                        .ok_or_else(|| "check result missing total".to_string())?,
                )
                .map_err(|_| "check total exceeds u32".to_string())?,
                checked: u32::try_from(
                    obj.get("checked")
                        .and_then(|v| v.as_u64())
                        .ok_or_else(|| "check result missing checked".to_string())?,
                )
                .map_err(|_| "check checked exceeds u32".to_string())?,
                unchecked,
                passed: obj
                    .get("passed")
                    .and_then(|v| v.as_bool())
                    .ok_or_else(|| "check result missing passed".to_string())?,
            })
        })
    }

    fn show_spec(
        req: exports::patina::slate::control::SpecIdRequest,
    ) -> Result<exports::patina::slate::control::ShowResult, String> {
        let project_root = resolve_project_root_from_hint(req.project.as_deref())?;
        with_project_root_cwd(&project_root, || {
            let value = handle_show(
                &project_root,
                Some(&serde_json::Map::from_iter([(
                    "id".to_string(),
                    serde_json::Value::String(req.id.clone()),
                )])),
            )?;
            let obj = value
                .as_object()
                .ok_or_else(|| "show result must be an object".to_string())?;

            let parse_string_vec = |key: &str| -> Result<Vec<String>, String> {
                obj.get(key)
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| format!("show result missing {}", key))?
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .ok_or_else(|| format!("show {} element must be string", key))
                            .map(|s| s.to_string())
                    })
                    .collect::<Result<Vec<_>, String>>()
            };

            let design_outline = obj
                .get("design_outline")
                .and_then(|v| v.as_array())
                .map(|values| {
                    values
                        .iter()
                        .map(|v| {
                            v.as_str()
                                .ok_or_else(|| {
                                    "show design_outline element must be string".to_string()
                                })
                                .map(|s| s.to_string())
                        })
                        .collect::<Result<Vec<_>, String>>()
                })
                .transpose()?;

            let frontmatter_json = serde_json::to_string(
                obj.get("frontmatter")
                    .ok_or_else(|| "show result missing frontmatter".to_string())?,
            )
            .map_err(|error| format!("serialize frontmatter: {}", error))?;

            Ok(exports::patina::slate::control::ShowResult {
                id: obj
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "show result missing id".to_string())?
                    .to_string(),
                frontmatter_json,
                outline: parse_string_vec("outline")?,
                design_outline,
                files: parse_string_vec("files")?,
                direct_code_targets: parse_string_vec("direct_code_targets")?,
                resolved_decisions: parse_string_vec("resolved_decisions")?,
                implementation_order: parse_string_vec("implementation_order")?,
                verification_points: parse_string_vec("verification_points")?,
                open_questions: parse_string_vec("open_questions")?,
                path: obj
                    .get("path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "show result missing path".to_string())?
                    .to_string(),
                design_path: obj
                    .get("design_path")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
            })
        })
    }

    fn prompt_spec(
        req: exports::patina::slate::control::SpecIdRequest,
    ) -> Result<exports::patina::slate::control::PromptResult, String> {
        let project_root = resolve_project_root_from_hint(req.project.as_deref())?;
        with_project_root_cwd(&project_root, || {
            let specs = load_specs(&project_root)?;
            let spec = find_spec(&specs, &req.id)?;
            let packet = build_prompt_packet(spec);
            let obj = packet
                .as_object()
                .ok_or_else(|| "prompt packet must be object".to_string())?;

            let parse_vec = |key: &str| -> Result<Vec<String>, String> {
                obj.get(key)
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| format!("prompt missing {}", key))?
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .ok_or_else(|| format!("prompt {} element must be string", key))
                            .map(|s| s.to_string())
                    })
                    .collect::<Result<Vec<_>, String>>()
            };

            Ok(exports::patina::slate::control::PromptResult {
                spec_id: obj
                    .get("spec_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "prompt missing spec_id".to_string())?
                    .to_string(),
                status: obj
                    .get("status")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "prompt missing status".to_string())?
                    .to_string(),
                title: obj
                    .get("title")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "prompt missing title".to_string())?
                    .to_string(),
                goal: obj
                    .get("goal")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "prompt missing goal".to_string())?
                    .to_string(),
                read_first: parse_vec("read_first")?,
                spec_path: obj
                    .get("spec_path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "prompt missing spec_path".to_string())?
                    .to_string(),
                design_path: obj
                    .get("design_path")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                direct_code_targets: parse_vec("direct_code_targets")?,
                execution_order: parse_vec("execution_order")?,
                constraints: parse_vec("constraints")?,
                verification: parse_vec("verification")?,
                definition_of_done: parse_vec("definition_of_done")?,
                session_workflow: parse_vec("session_workflow")?,
            })
        })
    }

    fn handoff_spec(
        req: exports::patina::slate::control::SpecIdRequest,
    ) -> Result<exports::patina::slate::control::HandoffResult, String> {
        let project_root = resolve_project_root_from_hint(req.project.as_deref())?;
        with_project_root_cwd(&project_root, || {
            let specs = load_specs(&project_root)?;
            let spec = find_spec(&specs, &req.id)?;
            let packet = build_handoff_packet(spec);
            let obj = packet
                .as_object()
                .ok_or_else(|| "handoff packet must be object".to_string())?;

            let parse_vec = |key: &str| -> Result<Vec<String>, String> {
                obj.get(key)
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| format!("handoff missing {}", key))?
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .ok_or_else(|| format!("handoff {} element must be string", key))
                            .map(|s| s.to_string())
                    })
                    .collect::<Result<Vec<_>, String>>()
            };

            let progress = obj
                .get("progress")
                .and_then(|v| v.as_object())
                .ok_or_else(|| "handoff missing progress".to_string())?;

            Ok(exports::patina::slate::control::HandoffResult {
                spec_id: obj
                    .get("spec_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "handoff missing spec_id".to_string())?
                    .to_string(),
                status: obj
                    .get("status")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "handoff missing status".to_string())?
                    .to_string(),
                title: obj
                    .get("title")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "handoff missing title".to_string())?
                    .to_string(),
                progress: exports::patina::slate::control::ProgressSummary {
                    checked: u32::try_from(
                        progress
                            .get("checked")
                            .and_then(|v| v.as_u64())
                            .ok_or_else(|| "handoff progress missing checked".to_string())?,
                    )
                    .map_err(|_| "handoff progress checked exceeds u32".to_string())?,
                    total: u32::try_from(
                        progress
                            .get("total")
                            .and_then(|v| v.as_u64())
                            .ok_or_else(|| "handoff progress missing total".to_string())?,
                    )
                    .map_err(|_| "handoff progress total exceeds u32".to_string())?,
                },
                resolved_decisions: parse_vec("resolved_decisions")?,
                completed_items: parse_vec("completed_items")?,
                open_items: parse_vec("open_items")?,
                next_steps: parse_vec("next_steps")?,
                verification: parse_vec("verification")?,
                spec_path: obj
                    .get("spec_path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "handoff missing spec_path".to_string())?
                    .to_string(),
                design_path: obj
                    .get("design_path")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
            })
        })
    }

    fn packet_spec(
        req: exports::patina::slate::control::SpecIdRequest,
    ) -> Result<exports::patina::slate::control::PacketResult, String> {
        let prompt = Self::prompt_spec(exports::patina::slate::control::SpecIdRequest {
            project: req.project.clone(),
            id: req.id.clone(),
        })?;
        let handoff = Self::handoff_spec(req)?;
        Ok(exports::patina::slate::control::PacketResult { prompt, handoff })
    }

    fn complete_spec(
        req: exports::patina::slate::control::CompleteRequest,
    ) -> Result<exports::patina::slate::control::CompleteResult, String> {
        let project_root = resolve_project_root_from_hint(req.project.as_deref())?;
        with_project_root_cwd(&project_root, || {
            let args = serde_json::Map::from_iter([
                ("id".to_string(), serde_json::Value::String(req.id.clone())),
                ("major".to_string(), serde_json::Value::Bool(req.major)),
                ("force".to_string(), serde_json::Value::Bool(req.force)),
            ]);
            let value = handle_complete(&project_root, Some(&args))?;
            let obj = value
                .as_object()
                .ok_or_else(|| "complete result must be an object".to_string())?;
            Ok(exports::patina::slate::control::CompleteResult {
                command: obj
                    .get("command")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "complete result missing command".to_string())?
                    .to_string(),
                spec_id: obj
                    .get("spec_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "complete result missing spec_id".to_string())?
                    .to_string(),
                new_status: obj
                    .get("new_status")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "complete result missing new_status".to_string())?
                    .to_string(),
                file: obj
                    .get("file")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "complete result missing file".to_string())?
                    .to_string(),
                tag: obj
                    .get("tag")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "complete result missing tag".to_string())?
                    .to_string(),
                archived: obj
                    .get("archived")
                    .and_then(|v| v.as_bool())
                    .ok_or_else(|| "complete result missing archived".to_string())?,
            })
        })
    }

    fn archive_spec(
        req: exports::patina::slate::control::ArchiveRequest,
    ) -> Result<exports::patina::slate::control::ArchiveResult, String> {
        let project_root = resolve_project_root_from_hint(req.project.as_deref())?;
        with_project_root_cwd(&project_root, || {
            let mut args = serde_json::Map::new();
            if let Some(id) = req.id.clone() {
                args.insert("id".to_string(), serde_json::Value::String(id));
            }
            args.insert("stale".to_string(), serde_json::Value::Bool(req.stale));
            args.insert("dry_run".to_string(), serde_json::Value::Bool(req.dry_run));

            let value = handle_archive(&project_root, Some(&args))?;
            let obj = value
                .as_object()
                .ok_or_else(|| "archive result must be an object".to_string())?;

            Ok(exports::patina::slate::control::ArchiveResult {
                id: obj
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|v| v.to_string()),
                stale: obj.get("stale").and_then(|v| v.as_bool()).unwrap_or(false),
                dry_run: obj
                    .get("dry_run")
                    .and_then(|v| v.as_bool())
                    .ok_or_else(|| "archive result missing dry_run".to_string())?,
            })
        })
    }

    fn dispatch(command_json: String) -> Result<String, String> {
        toys::measure::counter("slate_dispatch_calls", 1.0)?;

        let envelope: serde_json::Value = serde_json::from_str(&command_json)
            .map_err(|error| format!("invalid command_json: {}", error))?;
        let (command, backend_mode, project_root, data) = dispatch_data_from_envelope(&envelope)?;

        toys::measure::counter(&format!("slate_dispatch_command_{}", command), 1.0)?;

        toys::log::info(
            "slate-manager",
            &format!(
                "dispatch implemented command={} backend_mode={} project={} bytes={}",
                command,
                backend_mode,
                project_root.display(),
                command_json.len()
            ),
        );

        Ok(data.to_string())
    }
}
