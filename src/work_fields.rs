use crate::model::{SlateReleaseContract, WorkStatus};

pub(crate) fn set_work_field_schema() -> Vec<serde_json::Value> {
    let scalar_ops = ["set"];
    let optional_scalar_ops = ["set", "remove"];
    let list_ops = ["set", "add", "remove", "update:<one-based-index>"];

    vec![
        serde_json::json!({"field": "title", "kind": "string", "operations": scalar_ops}),
        serde_json::json!({"field": "status", "kind": "string", "operations": scalar_ops}),
        serde_json::json!({"field": "human_request", "kind": "string", "operations": scalar_ops}),
        serde_json::json!({"field": "user_value", "kind": "string", "operations": scalar_ops, "aliases": ["user-value", "value"]}),
        serde_json::json!({"field": "scope", "kind": "list<string>", "operations": list_ops}),
        serde_json::json!({"field": "non_goals", "kind": "list<string>", "operations": list_ops, "aliases": ["non-goals", "non_goal", "non-goal"]}),
        serde_json::json!({"field": "stop_condition", "kind": "string", "operations": scalar_ops, "aliases": ["stop-condition"]}),
        serde_json::json!({"field": "body", "kind": "markdown", "operations": ["set", "add", "remove"], "aliases": ["narrative", "story", "work_md", "work-md"]}),
        serde_json::json!({"field": "target", "kind": "option<string>", "operations": optional_scalar_ops}),
        serde_json::json!({"field": "user_alignment", "kind": "string", "operations": scalar_ops}),
        serde_json::json!({"field": "belief_harvest_decision", "kind": "option<string>", "operations": optional_scalar_ops}),
        serde_json::json!({"field": "proof_plan", "kind": "list<string>", "operations": list_ops}),
        serde_json::json!({"field": "implementation_plan", "kind": "list<string>", "operations": list_ops}),
        serde_json::json!({"field": "closure_evidence", "kind": "list<string>", "operations": list_ops}),
        serde_json::json!({"field": "blocked_by", "kind": "list<string>", "operations": list_ops}),
        serde_json::json!({"field": "blocks", "kind": "list<string>", "operations": list_ops}),
        serde_json::json!({"field": "release_contract", "kind": "json<object>", "operations": optional_scalar_ops}),
        serde_json::json!({"field": "allium_anchor", "kind": "list<string>", "operations": list_ops, "aliases": ["allium_anchors", "allium-anchors"]}),
        serde_json::json!({"field": "belief_ref", "kind": "list<string>", "operations": list_ops, "aliases": ["belief_refs", "belief-refs"]}),
    ]
}

pub(crate) fn valid_set_work_fields() -> Vec<&'static str> {
    vec![
        "title",
        "status",
        "human_request",
        "user_value",
        "scope",
        "non_goals",
        "stop_condition",
        "body",
        "target",
        "user_alignment",
        "belief_harvest_decision",
        "proof_plan",
        "implementation_plan",
        "closure_evidence",
        "blocked_by",
        "blocks",
        "release_contract",
        "allium_anchor",
        "belief_ref",
    ]
}

pub(crate) fn normalize_set_work_field(field: &str) -> Option<&'static str> {
    match field {
        "title" => Some("title"),
        "status" => Some("status"),
        "human_request" | "human-request" => Some("human_request"),
        "user_value" | "user-value" | "value" => Some("user_value"),
        "scope" => Some("scope"),
        "non_goals" | "non-goals" | "non_goal" | "non-goal" => Some("non_goals"),
        "stop_condition" | "stop-condition" => Some("stop_condition"),
        "body" | "narrative" | "story" | "work_md" | "work-md" => Some("body"),
        "target" => Some("target"),
        "user_alignment" | "user-alignment" => Some("user_alignment"),
        "belief_harvest_decision" | "belief-harvest-decision" => Some("belief_harvest_decision"),
        "proof_plan" | "proof-plan" => Some("proof_plan"),
        "implementation_plan" | "implementation-plan" => Some("implementation_plan"),
        "closure_evidence" | "closure-evidence" => Some("closure_evidence"),
        "blocked_by" | "blocked-by" => Some("blocked_by"),
        "blocks" => Some("blocks"),
        "release_contract" | "release-contract" => Some("release_contract"),
        "allium_anchor" | "allium-anchor" | "allium_anchors" | "allium-anchors" => {
            Some("allium_anchor")
        }
        "belief_ref" | "belief-ref" | "belief_refs" | "belief-refs" => Some("belief_ref"),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SetWorkOperation {
    Default,
    Set,
    Add,
    Remove,
    Update(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SetWorkFieldSpec {
    pub(crate) field: &'static str,
    pub(crate) operation: SetWorkOperation,
}

pub(crate) fn parse_set_work_field_spec(raw_field: &str) -> Result<SetWorkFieldSpec, String> {
    let mut parts = raw_field.split(':');
    let base = parts.next().unwrap_or_default();
    let operation = parts.next();
    let index = parts.next();
    if parts.next().is_some() {
        return Err(format!(
            "invalid Slate field operation '{}'. Use '<field>', '<field>:set', '<field>:add', '<field>:remove', or '<field>:update:<index>'",
            raw_field
        ));
    }

    let field =
        normalize_set_work_field(base).ok_or_else(|| unsupported_set_work_field_error(base))?;
    let operation = match operation {
        None => SetWorkOperation::Default,
        Some("set" | "replace") => SetWorkOperation::Set,
        Some("add" | "append") => SetWorkOperation::Add,
        Some("remove" | "delete") => SetWorkOperation::Remove,
        Some("update") => {
            let raw_index = index.ok_or_else(|| {
                format!(
                    "missing update index for '{}'. Example: field='proof_plan:update:1' value='[x] cargo test'",
                    raw_field
                )
            })?;
            let parsed = raw_index.parse::<usize>().map_err(|_| {
                format!(
                    "invalid update index '{}' for '{}': use a one-based integer",
                    raw_index, raw_field
                )
            })?;
            if parsed == 0 {
                return Err(format!(
                    "invalid update index '{}' for '{}': indexes are one-based",
                    raw_index, raw_field
                ));
            }
            SetWorkOperation::Update(parsed)
        }
        Some(other) => {
            return Err(format!(
                "unsupported Slate field operation '{}'. Valid operations: set, add, remove, update:<index>",
                other
            ))
        }
    };

    Ok(SetWorkFieldSpec { field, operation })
}

pub(crate) fn unsupported_set_work_field_error(field: &str) -> String {
    let valid = valid_set_work_fields().join(", ");
    format!(
        "unsupported Slate field '{}'. Valid fields: {}. Examples: field='proof_plan:add' value='[ ] observable proof criterion'; field='allium_anchors:set' value='[\"layer/core/spec-driven-design.md\"]'",
        field, valid
    )
}

pub(crate) fn parse_list_items(value: &str) -> Vec<String> {
    if let Ok(items) = serde_json::from_str::<Vec<String>>(value) {
        return items
            .into_iter()
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect();
    }

    let lines = value
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if lines.is_empty() && !value.trim().is_empty() {
        vec![value.trim().to_string()]
    } else {
        lines
    }
}

pub(crate) fn apply_required_string_field(
    field: &str,
    slot: &mut String,
    operation: &SetWorkOperation,
    value: String,
) -> Result<(), String> {
    match operation {
        SetWorkOperation::Default | SetWorkOperation::Set => {
            *slot = value;
            Ok(())
        }
        SetWorkOperation::Remove => Err(format!(
            "cannot remove required Slate field '{}'; use '{}:set' with a replacement value",
            field, field
        )),
        SetWorkOperation::Add | SetWorkOperation::Update(_) => Err(format!(
            "operation not supported for scalar Slate field '{}'; use '{}:set'",
            field, field
        )),
    }
}

pub(crate) fn apply_status_field(
    slot: &mut WorkStatus,
    operation: &SetWorkOperation,
    value: String,
) -> Result<(), String> {
    match operation {
        SetWorkOperation::Default | SetWorkOperation::Set => {
            *slot = value.parse::<WorkStatus>()?;
            Ok(())
        }
        SetWorkOperation::Remove => Err(
            "cannot remove required Slate field 'status'; use 'status:set' with a replacement value"
                .to_string(),
        ),
        SetWorkOperation::Add | SetWorkOperation::Update(_) => Err(
            "operation not supported for scalar Slate field 'status'; use 'status:set'".to_string(),
        ),
    }
}

pub(crate) fn apply_optional_string_field(
    field: &str,
    slot: &mut Option<String>,
    operation: &SetWorkOperation,
    value: String,
) -> Result<(), String> {
    match operation {
        SetWorkOperation::Default | SetWorkOperation::Set => {
            *slot = Some(value);
            Ok(())
        }
        SetWorkOperation::Remove => {
            *slot = None;
            Ok(())
        }
        SetWorkOperation::Add | SetWorkOperation::Update(_) => Err(format!(
            "operation not supported for optional scalar Slate field '{}'; use '{}:set' or '{}:remove'",
            field, field, field
        )),
    }
}

pub(crate) fn remove_list_items(
    field: &str,
    slot: &mut Vec<String>,
    value: &str,
) -> Result<(), String> {
    if let Ok(index) = value.trim().parse::<usize>() {
        if index == 0 || index > slot.len() {
            return Err(format!(
                "cannot remove {} item {}: valid range is 1..={}",
                field,
                index,
                slot.len()
            ));
        }
        slot.remove(index - 1);
        return Ok(());
    }

    let removals = parse_list_items(value);
    let before = slot.len();
    slot.retain(|item| !removals.iter().any(|remove| item.trim() == remove.trim()));
    if slot.len() == before {
        return Err(format!(
            "no {} items matched removal value. Use an exact value or one-based index",
            field
        ));
    }
    Ok(())
}

pub(crate) fn apply_list_field(
    field: &str,
    slot: &mut Vec<String>,
    operation: &SetWorkOperation,
    value: String,
) -> Result<(), String> {
    match operation {
        SetWorkOperation::Default | SetWorkOperation::Add => {
            slot.extend(parse_list_items(&value));
            Ok(())
        }
        SetWorkOperation::Set => {
            *slot = parse_list_items(&value);
            Ok(())
        }
        SetWorkOperation::Remove => remove_list_items(field, slot, &value),
        SetWorkOperation::Update(index) => {
            if *index > slot.len() {
                return Err(format!(
                    "cannot update {} item {}: valid range is 1..={}",
                    field,
                    index,
                    slot.len()
                ));
            }
            slot[*index - 1] = value.trim().to_string();
            Ok(())
        }
    }
}

pub(crate) fn valid_release_ecosystems() -> &'static [&'static str] {
    &[
        "rust",
        "node",
        "typescript",
        "go",
        "java",
        "clojure",
        "c",
        "custom",
    ]
}

pub(crate) fn valid_release_version_strategies() -> &'static [&'static str] {
    &[
        "cargo",
        "npm",
        "pnpm",
        "yarn",
        "bun",
        "go-module",
        "maven",
        "gradle",
        "deps-edn",
        "lein",
        "make",
        "cmake",
        "custom",
    ]
}

pub(crate) fn validate_release_contract(contract: &SlateReleaseContract) -> Result<(), String> {
    if contract.units.is_empty() {
        return Err("release_contract.units must contain at least one release unit".to_string());
    }

    let ecosystems = valid_release_ecosystems();
    let strategies = valid_release_version_strategies();
    for (idx, unit) in contract.units.iter().enumerate() {
        let label = if unit.name.trim().is_empty() {
            format!("unit[{}]", idx)
        } else {
            format!("unit '{}'", unit.name)
        };
        if unit.name.trim().is_empty() {
            return Err(format!("release_contract {} requires name", label));
        }
        if !ecosystems.contains(&unit.ecosystem.as_str()) {
            return Err(format!(
                "release_contract {} has unsupported ecosystem '{}'. Valid ecosystems: {}",
                label,
                unit.ecosystem,
                ecosystems.join(", ")
            ));
        }
        if !strategies.contains(&unit.version_strategy.as_str()) {
            return Err(format!(
                "release_contract {} has unsupported version_strategy '{}'. Valid strategies: {}",
                label,
                unit.version_strategy,
                strategies.join(", ")
            ));
        }
        if let Some(bump) = unit.bump_type.as_deref() {
            if !matches!(bump, "patch" | "minor" | "major") {
                return Err(format!(
                    "release_contract {} has invalid bump_type '{}': expected patch, minor, or major",
                    label, bump
                ));
            }
        }
        if unit.version_files.is_empty() && unit.version_strategy != "custom" {
            return Err(format!(
                "release_contract {} requires version_files unless version_strategy is custom",
                label
            ));
        }
    }
    Ok(())
}

pub(crate) fn parse_release_contract(value: &str) -> Result<SlateReleaseContract, String> {
    let contract: SlateReleaseContract = serde_json::from_str(value).map_err(|error| {
        format!(
            "invalid release_contract JSON: {}. Expected object with release_tag, changelog_updated, and units [{{name, ecosystem, version_strategy, bump_type, version_files, artifact_build_command, verification}}]",
            error
        )
    })?;
    validate_release_contract(&contract)?;
    Ok(contract)
}

pub(crate) fn apply_release_contract_field(
    slot: &mut Option<SlateReleaseContract>,
    operation: &SetWorkOperation,
    value: String,
) -> Result<(), String> {
    match operation {
        SetWorkOperation::Default | SetWorkOperation::Set => {
            *slot = Some(parse_release_contract(&value)?);
            Ok(())
        }
        SetWorkOperation::Remove => {
            *slot = None;
            Ok(())
        }
        SetWorkOperation::Add | SetWorkOperation::Update(_) => Err(
            "operation not supported for release_contract; use release_contract:set or release_contract:remove"
                .to_string(),
        ),
    }
}

pub(crate) fn release_contract_schema() -> serde_json::Value {
    serde_json::json!({
        "ownership": "Slate records the release contract; project/tooling owns language-specific version mutation, build, tag, and publishing mechanics.",
        "shape": {
            "release_tag": "optional intended release tag such as v0.2.0",
            "changelog_updated": "bool evidence that release notes/changelog were updated",
            "units": [
                {
                    "name": "release unit name, package, service, crate, module, app, or component",
                    "ecosystem": valid_release_ecosystems(),
                    "version_strategy": valid_release_version_strategies(),
                    "bump_type": "optional patch|minor|major",
                    "version_files": "list of project-owned version metadata files for this unit",
                    "artifact_build_command": "optional project-owned build command proving artifacts for this unit",
                    "verification": "list of unit-specific commands/evidence checked before release",
                }
            ]
        },
        "examples": [
            {
                "release_tag": "v0.2.0",
                "changelog_updated": true,
                "units": [
                    {
                        "name": "slate-manager",
                        "ecosystem": "rust",
                        "version_strategy": "cargo",
                        "bump_type": "minor",
                        "version_files": ["Cargo.toml"],
                        "artifact_build_command": "cargo component build --release",
                        "verification": ["cargo test --all-targets"],
                    },
                    {
                        "name": "web-client",
                        "ecosystem": "typescript",
                        "version_strategy": "pnpm",
                        "bump_type": "minor",
                        "version_files": ["package.json"],
                        "artifact_build_command": "pnpm build",
                        "verification": ["pnpm test"],
                    },
                    {
                        "name": "worker",
                        "ecosystem": "go",
                        "version_strategy": "go-module",
                        "bump_type": "patch",
                        "version_files": ["go.mod"],
                        "artifact_build_command": "go build ./...",
                        "verification": ["go test ./..."],
                    }
                ]
            }
        ]
    })
}

pub(crate) fn handle_schema() -> serde_json::Value {
    serde_json::json!({
        "work": {
            "mutable_fields": set_work_field_schema(),
            "set_work_field_syntax": [
                "<field> (back-compatible default: scalar set, list add)",
                "<field>:set",
                "<field>:add",
                "<field>:remove",
                "<field>:update:<one-based-index>",
            ],
            "release_contract": release_contract_schema(),
            "transitions": [
                {
                    "from": "draft",
                    "to": "ready",
                    "command": "promote-work",
                    "gates": [
                        "kind present",
                        "human_request present",
                        "user_alignment present",
                        "proof_plan non-empty",
                        "allium_anchors present OR refactor includes no-behavior rationale",
                    ],
                },
                {
                    "from": "ready",
                    "to": "active",
                    "command": "promote-work",
                    "gates": ["none"],
                },
                {
                    "from": "draft|ready",
                    "to": "active",
                    "command": "activate-work",
                    "gates": [
                        "draft uses the same ready gates as promote-work",
                        "ready has no additional gates",
                        "history event payload records from/to/force",
                    ],
                },
                {
                    "from": "active",
                    "to": "complete",
                    "command": "complete-work",
                    "gates": [
                        "proof_plan fully checked",
                        "closure_evidence present",
                        "belief_harvest_decision present",
                    ],
                }
            ]
        }
    })
}
