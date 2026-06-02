use crate::dispatch::dispatch_for_test;
use crate::lifecycle::{activate_slate_work, validate_ready_gate};
use crate::model::{SlateWorkFile, WorkKind, WorkStatus};
use crate::runtime::resolve_project_root_from_hint;
use crate::store::{
    create_slate_work_file, find_slate_work, load_slate_events, load_slate_work, update_slate_work,
};
use crate::work_fields::{
    apply_list_field, handle_schema, normalize_set_work_field, parse_release_contract,
    parse_set_work_field_spec, unsupported_set_work_field_error, SetWorkFieldSpec,
    SetWorkOperation,
};
use crate::work_views::{validate_complete_gate, work_handoff_result, work_prompt_result};
use crate::{exports, SlateManager};
use std::fs;

fn temp_project() -> tempfile::TempDir {
    let temp = tempfile::tempdir().expect("temp project");
    fs::create_dir_all(temp.path().join(".patina")).expect("patina dir");
    fs::create_dir_all(temp.path().join("layer")).expect("layer dir");
    temp
}

#[test]
fn project_hint_does_not_remap_host_absolute_paths_to_input() {
    let temp = temp_project();
    let host_absolute = temp.path().to_string_lossy().to_string();
    let error =
        resolve_project_root_from_hint(Some(&format!("/missing-guest-prefix{}", host_absolute)))
            .expect_err("host absolute paths must not be remapped by Slate");
    assert!(error.contains("Patina/Mother must mount"), "{error}");
    assert!(!error.contains("/input/"), "{error}");
}

#[test]
fn native_slate_create_update_and_history_are_project_living() {
    let temp = temp_project();
    let mut work = SlateWorkFile {
        id: "demo".to_string(),
        title: "Demo".to_string(),
        kind: WorkKind::Build,
        status: WorkStatus::Draft,
        human_request: "Build it".to_string(),
        allium_anchors: vec!["layer/allium/demo.allium".to_string()],
        user_alignment: "User confirmed".to_string(),
        proof_plan: vec!["[x] cargo test".to_string()],
        ..Default::default()
    };

    let created = create_slate_work_file(temp.path(), &mut work).expect("create work");
    assert_eq!(created.path, "layer/slate/work/demo/work.toml");

    let promoted = update_slate_work(
        temp.path(),
        "demo",
        "promoted",
        serde_json::json!({"to": "ready"}),
        |work| {
            validate_ready_gate(work)?;
            work.status = WorkStatus::Ready;
            Ok(())
        },
    )
    .expect("promote");
    assert_eq!(promoted.work.status, "ready");

    let events = load_slate_events(temp.path(), "demo").expect("events");
    assert_eq!(events.len(), 2);
    assert!(temp.path().join("layer/slate/work/demo/work.toml").exists());
    assert!(temp.path().join("layer/slate/work/demo/work.md").exists());
    assert!(temp.path().join("layer/slate/events.jsonl").exists());
}

#[test]
fn native_slate_status_and_kind_round_trip_as_toml_strings() {
    let work = SlateWorkFile {
        id: "demo".to_string(),
        title: "Demo".to_string(),
        kind: WorkKind::Refactor,
        status: WorkStatus::Blocked,
        human_request: "Refactor it".to_string(),
        user_alignment: "User confirmed".to_string(),
        ..Default::default()
    };

    let raw = toml::to_string(&work).expect("serialize work");
    assert!(raw.contains("kind = \"refactor\""), "{raw}");
    assert!(raw.contains("status = \"blocked\""), "{raw}");

    let parsed: SlateWorkFile = toml::from_str(&raw).expect("parse work");
    assert_eq!(parsed.kind, WorkKind::Refactor);
    assert_eq!(parsed.status, WorkStatus::Blocked);

    let err = toml::from_str::<SlateWorkFile>(
        "id='bad'\ntitle='Bad'\nkind='build'\nstatus='bananas'\nhuman_request='x'\n",
    )
    .expect_err("unknown status should not parse");
    assert!(err.to_string().contains("unknown variant"), "{err}");
}

#[test]
fn native_slate_ready_gate_blocks_missing_intent() {
    let work = SlateWorkFile {
        id: "demo".to_string(),
        title: "Demo".to_string(),
        kind: WorkKind::Build,
        human_request: "Build it".to_string(),
        user_alignment: "User confirmed".to_string(),
        proof_plan: vec!["cargo test".to_string()],
        ..Default::default()
    };
    let err = validate_ready_gate(&work).expect_err("missing allium should block");
    assert!(err.contains("Allium"));
}

#[test]
fn native_slate_packet_and_completion_gates_use_work_artifacts() {
    let temp = temp_project();
    let mut work = SlateWorkFile {
        id: "demo".to_string(),
        title: "Demo".to_string(),
        kind: WorkKind::Build,
        status: WorkStatus::Active,
        human_request: "Build it".to_string(),
        allium_anchors: vec!["layer/allium/demo.allium".to_string()],
        user_alignment: "User confirmed".to_string(),
        implementation_plan: vec!["edit src/lib.rs".to_string()],
        proof_plan: vec!["[x] cargo test".to_string()],
        belief_refs: vec!["[[dependable-rust]]".to_string()],
        closure_evidence: vec!["cargo test passed".to_string()],
        belief_harvest_decision: Some("no belief change".to_string()),
        ..Default::default()
    };
    let record = create_slate_work_file(temp.path(), &mut work).expect("create work");
    let records = load_slate_work(temp.path()).expect("records");
    let prompt = work_prompt_result(temp.path(), &records, record.clone());
    assert_eq!(prompt.work_id, "demo");
    assert!(prompt.read_first.iter().any(|item| item == "layer/allium/"));
    assert!(prompt
        .read_first
        .iter()
        .any(|item| item == "layer/slate/work/demo/work.md"));
    assert!(prompt.narrative_summary.contains("Story:"));
    let handoff = work_handoff_result(temp.path(), &records, record.clone()).expect("handoff");
    assert_eq!(handoff.progress.checked, 1);
    validate_complete_gate(&record.work).expect("complete gate");

    let incomplete = SlateWorkFile {
        id: "bad".to_string(),
        title: "Bad".to_string(),
        kind: WorkKind::Build,
        status: WorkStatus::Active,
        human_request: "Build it".to_string(),
        allium_anchors: vec!["layer/allium/demo.allium".to_string()],
        user_alignment: "User confirmed".to_string(),
        proof_plan: vec!["[x] cargo test".to_string()],
        ..Default::default()
    };
    let err = validate_complete_gate(&incomplete).expect_err("missing closure should block");
    assert!(err.contains("closure evidence"));
}

#[test]
fn native_slate_set_work_field_schema_and_aliases_are_discoverable() {
    let schema = handle_schema();
    let fields = schema
        .get("work")
        .and_then(|work| work.get("mutable_fields"))
        .and_then(|value| value.as_array())
        .expect("schema mutable_fields");
    assert!(fields.iter().any(|f| {
        f.get("field").and_then(|v| v.as_str()) == Some("allium_anchor")
            && f.get("aliases").is_some()
    }));

    assert_eq!(
        normalize_set_work_field("allium_anchors"),
        Some("allium_anchor")
    );
    assert_eq!(normalize_set_work_field("belief_refs"), Some("belief_ref"));
    assert_eq!(normalize_set_work_field("story"), Some("body"));
}

#[test]
fn native_slate_set_work_invalid_field_error_is_actionable() {
    let err = unsupported_set_work_field_error("allium_anchorz");
    assert!(err.contains("Valid fields:"));
    assert!(err.contains("Examples:"));
    assert!(err.contains("proof_plan"));
}

#[test]
fn native_slate_set_work_parses_operations_and_mutates_lists() {
    assert_eq!(
        parse_set_work_field_spec("proof_plan:update:2").expect("field spec"),
        SetWorkFieldSpec {
            field: "proof_plan",
            operation: SetWorkOperation::Update(2),
        }
    );
    assert_eq!(
        normalize_set_work_field("human-request"),
        Some("human_request")
    );

    let mut items = vec!["[ ] first".to_string(), "[ ] second".to_string()];
    apply_list_field(
        "proof_plan",
        &mut items,
        &SetWorkOperation::Update(2),
        "[x] second".to_string(),
    )
    .expect("update item");
    assert_eq!(items[1], "[x] second");

    apply_list_field(
        "proof_plan",
        &mut items,
        &SetWorkOperation::Add,
        "[ ] third\n[ ] fourth".to_string(),
    )
    .expect("add items");
    assert_eq!(items.len(), 4);

    apply_list_field(
        "proof_plan",
        &mut items,
        &SetWorkOperation::Remove,
        "1".to_string(),
    )
    .expect("remove item");
    assert_eq!(items[0], "[x] second");
}

#[test]
fn native_slate_set_work_api_updates_human_request_and_allium_anchors() {
    let temp = temp_project();
    let mut work = SlateWorkFile {
        id: "demo".to_string(),
        title: "Demo".to_string(),
        kind: WorkKind::Build,
        status: WorkStatus::Draft,
        human_request: "Initial".to_string(),
        user_alignment: "User confirmed".to_string(),
        ..Default::default()
    };
    create_slate_work_file(temp.path(), &mut work).expect("create work");
    let project = Some(temp.path().to_string_lossy().to_string());

    let updated = <SlateManager as exports::patina::slate::control::Guest>::set_work(
        exports::patina::slate::control::SetWorkRequest {
            project: project.clone(),
            id: "demo".to_string(),
            field: "human-request:set".to_string(),
            value: "Updated request".to_string(),
        },
    )
    .expect("set human request");
    assert_eq!(updated.human_request, "Updated request");

    let updated = <SlateManager as exports::patina::slate::control::Guest>::set_work(
        exports::patina::slate::control::SetWorkRequest {
            project: project.clone(),
            id: "demo".to_string(),
            field: "allium_anchors:set".to_string(),
            value: "[\"layer/core/spec-driven-design.md\",\"layer/core/unix-philosophy.md\"]"
                .to_string(),
        },
    )
    .expect("set allium anchors");
    assert_eq!(updated.allium_anchors.len(), 2);

    let updated = <SlateManager as exports::patina::slate::control::Guest>::set_work(
        exports::patina::slate::control::SetWorkRequest {
            project,
            id: "demo".to_string(),
            field: "proof_plan:set".to_string(),
            value: "[ ] first\n[ ] second".to_string(),
        },
    )
    .expect("replace proof plan");
    assert_eq!(updated.proof_plan.len(), 2);
}

#[test]
fn native_slate_body_loads_context_and_dispatch_reads_work_store() {
    let temp = temp_project();
    fs::create_dir_all(temp.path().join("layer/surface")).expect("surface dir");
    fs::write(
        temp.path().join("layer/surface/adapter-mapping.md"),
        "# Adapter Mapping\n\n## Purpose\nExplain why adapter mapping drives this work.",
    )
    .expect("context doc");

    let mut work = SlateWorkFile {
        id: "demo".to_string(),
        title: "Demo".to_string(),
        kind: WorkKind::Build,
        status: WorkStatus::Draft,
        human_request: "Initial".to_string(),
        user_alignment: "User confirmed".to_string(),
        ..Default::default()
    };
    create_slate_work_file(temp.path(), &mut work).expect("create work");
    let project = Some(temp.path().to_string_lossy().to_string());

    let updated = <SlateManager as exports::patina::slate::control::Guest>::set_work(
        exports::patina::slate::control::SetWorkRequest {
            project: project.clone(),
            id: "demo".to_string(),
            field: "body:set".to_string(),
            value: "## Story\nThis work fixes opaque handoffs.\n\n## Why\nThe story must be writable.\n\n## Context\nSee [adapter mapping](layer/surface/adapter-mapping.md).".to_string(),
        },
    )
    .expect("set body");
    assert_eq!(
        updated.body_path.as_deref(),
        Some("layer/slate/work/demo/work.md")
    );
    assert!(updated.narrative_summary.contains("opaque handoffs"));

    let records = load_slate_work(temp.path()).expect("records");
    let record = find_slate_work(&records, "demo").expect("demo").clone();
    let prompt = work_prompt_result(temp.path(), &records, record);
    assert!(prompt
        .narrative_context
        .iter()
        .any(|item| item.contains("Adapter Mapping")));

    let show = dispatch_for_test(
        &serde_json::json!({
            "project": temp.path().to_string_lossy(),
            "command": {"show": {"id": "demo"}}
        })
        .to_string(),
    )
    .expect("dispatch show");
    assert_eq!(show.get("id").and_then(|v| v.as_str()), Some("demo"));
    assert!(show
        .get("narrative_context")
        .and_then(|v| v.as_array())
        .expect("context")
        .iter()
        .any(|item| item
            .as_str()
            .is_some_and(|text| text.contains("Adapter Mapping"))));

    let next = dispatch_for_test(
        &serde_json::json!({
            "project": temp.path().to_string_lossy(),
            "command": {"next": {}}
        })
        .to_string(),
    )
    .expect("dispatch next");
    assert!(next
        .as_array()
        .expect("next array")
        .iter()
        .any(|item| item.get("id").and_then(|v| v.as_str()) == Some("demo")));
}

#[test]
fn native_slate_dependency_reconcile_is_bidirectional_and_unblocks_completed_slice() {
    let temp = temp_project();
    let project = Some(temp.path().to_string_lossy().to_string());
    let mut slice = SlateWorkFile {
        id: "slice".to_string(),
        title: "Slice".to_string(),
        kind: WorkKind::Build,
        status: WorkStatus::Active,
        human_request: "Land the slice".to_string(),
        allium_anchors: vec!["layer/core/spec-driven-design.md".to_string()],
        user_alignment: "User confirmed".to_string(),
        proof_plan: vec!["[x] cargo test".to_string()],
        closure_evidence: vec!["cargo test passed".to_string()],
        belief_harvest_decision: Some("no belief change".to_string()),
        ..Default::default()
    };
    create_slate_work_file(temp.path(), &mut slice).expect("create slice");

    let mut epic = SlateWorkFile {
        id: "epic".to_string(),
        title: "Epic".to_string(),
        kind: WorkKind::Build,
        status: WorkStatus::Blocked,
        human_request: "Continue the epic".to_string(),
        user_alignment: "User confirmed".to_string(),
        blocked_by: vec!["slice".to_string()],
        block_reason: Some("waiting on slice".to_string()),
        ..Default::default()
    };
    create_slate_work_file(temp.path(), &mut epic).expect("create epic");

    let completed = <SlateManager as exports::patina::slate::control::Guest>::complete_work(
        exports::patina::slate::control::WorkStatusRequest {
            project,
            id: "slice".to_string(),
            force: false,
        },
    )
    .expect("complete slice");
    assert!(completed.blocks.contains(&"epic".to_string()));

    let records = load_slate_work(temp.path()).expect("records");
    let slice = find_slate_work(&records, "slice").expect("slice");
    let epic = find_slate_work(&records, "epic").expect("epic");
    assert!(slice.work.blocks.contains(&"epic".to_string()));
    assert!(epic.work.blocked_by.contains(&"slice".to_string()));
    assert_eq!(epic.work.status, "active");
    assert!(epic.work.block_reason.is_none());

    let events = load_slate_events(temp.path(), "epic").expect("events");
    assert!(events.iter().any(|event| {
        event.get("event_type").and_then(|value| value.as_str()) == Some("dependencies-reconciled")
            && event
                .get("payload")
                .and_then(|payload| payload.get("unblocked"))
                .and_then(|value| value.as_bool())
                == Some(true)
    }));
}

#[test]
fn native_slate_activate_work_is_single_explicit_transition() {
    let temp = temp_project();
    let mut work = SlateWorkFile {
        id: "demo".to_string(),
        title: "Demo".to_string(),
        kind: WorkKind::Build,
        status: WorkStatus::Draft,
        human_request: "Build it".to_string(),
        allium_anchors: vec!["layer/core/spec-driven-design.md".to_string()],
        user_alignment: "User confirmed".to_string(),
        proof_plan: vec!["[ ] cargo test".to_string()],
        ..Default::default()
    };
    create_slate_work_file(temp.path(), &mut work).expect("create work");

    let activated = activate_slate_work(temp.path(), "demo", false).expect("activate");
    assert_eq!(activated.work.status, "active");

    let events = load_slate_events(temp.path(), "demo").expect("events");
    let payload = events
        .last()
        .and_then(|event| event.get("payload"))
        .expect("activation payload");
    assert_eq!(payload.get("from").and_then(|v| v.as_str()), Some("draft"));
    assert_eq!(payload.get("to").and_then(|v| v.as_str()), Some("active"));
}

#[test]
fn native_slate_ready_gate_errors_list_missing_gates() {
    let work = SlateWorkFile {
        id: "demo".to_string(),
        title: "Demo".to_string(),
        kind: WorkKind::Build,
        ..Default::default()
    };
    let err = validate_ready_gate(&work).expect_err("missing gates");
    assert!(err.contains("Missing gates:"));
    assert!(err.contains("human_request"));
    assert!(err.contains("proof_plan"));
    assert!(err.contains("allium_anchors"));
}

#[test]
fn native_slate_release_contract_is_schema_visible_and_language_agnostic() {
    let schema = handle_schema();
    let release_contract = schema
        .get("work")
        .and_then(|work| work.get("release_contract"))
        .expect("release contract schema");
    assert!(release_contract
        .get("ownership")
        .and_then(|value| value.as_str())
        .expect("ownership")
        .contains("project/tooling owns language-specific"));
    let examples = release_contract
        .get("examples")
        .and_then(|value| value.as_array())
        .expect("examples");
    let example_text = examples[0].to_string();
    assert!(example_text.contains("typescript"));
    assert!(example_text.contains("go-module"));
    assert!(example_text.contains("cargo"));
}

#[test]
fn native_slate_set_work_api_sets_and_removes_release_contract() {
    let temp = temp_project();
    let mut work = SlateWorkFile {
        id: "demo".to_string(),
        title: "Demo".to_string(),
        kind: WorkKind::Build,
        status: WorkStatus::Draft,
        human_request: "Release it".to_string(),
        user_alignment: "User confirmed".to_string(),
        ..Default::default()
    };
    create_slate_work_file(temp.path(), &mut work).expect("create work");
    let project = Some(temp.path().to_string_lossy().to_string());

    let contract = serde_json::json!({
        "changelog_updated": true,
        "release_tag": "v0.2.0",
        "units": [
            {
                "name": "slate-manager",
                "ecosystem": "rust",
                "version_strategy": "cargo",
                "bump_type": "minor",
                "version_files": ["Cargo.toml"],
                "artifact_build_command": "cargo component build --release",
                "verification": ["cargo test --all-targets"]
            },
            {
                "name": "web-client",
                "ecosystem": "typescript",
                "version_strategy": "pnpm",
                "bump_type": "minor",
                "version_files": ["package.json"],
                "artifact_build_command": "pnpm build",
                "verification": ["pnpm test"]
            },
            {
                "name": "worker",
                "ecosystem": "go",
                "version_strategy": "go-module",
                "bump_type": "patch",
                "version_files": ["go.mod"],
                "artifact_build_command": "go build ./...",
                "verification": ["go test ./..."]
            }
        ]
    });
    let updated = <SlateManager as exports::patina::slate::control::Guest>::set_work(
        exports::patina::slate::control::SetWorkRequest {
            project: project.clone(),
            id: "demo".to_string(),
            field: "release-contract:set".to_string(),
            value: contract.to_string(),
        },
    )
    .expect("set release contract");
    let release_contract_json = updated.release_contract_json.expect("contract json");
    assert!(release_contract_json.contains("Cargo.toml"));
    assert!(release_contract_json.contains("package.json"));
    assert!(release_contract_json.contains("go.mod"));

    let updated = <SlateManager as exports::patina::slate::control::Guest>::set_work(
        exports::patina::slate::control::SetWorkRequest {
            project,
            id: "demo".to_string(),
            field: "release_contract:remove".to_string(),
            value: String::new(),
        },
    )
    .expect("remove release contract");
    assert!(updated.release_contract_json.is_none());
}

#[test]
fn native_slate_release_contract_rejects_unknown_ecosystem() {
    let err = parse_release_contract(
        &serde_json::json!({
            "units": [{
                "name": "mystery",
                "ecosystem": "unknown-lang",
                "version_strategy": "custom"
            }]
        })
        .to_string(),
    )
    .expect_err("unknown ecosystem should fail");
    assert!(err.contains("unsupported ecosystem"));
}
