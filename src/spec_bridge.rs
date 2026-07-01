use crate::model::{ExitCriterionLite, SpecFrontmatterLite, SpecRecord};
use crate::patina::git::git;
use crate::runtime::{arg_bool, arg_string, require_id, to_repo_relative};
use crate::text::{
    extract_code_targets, extract_key_files, extract_outline, extract_section_items,
    extract_section_paragraph, extract_title, slugify,
};
use patina_sdk::toys;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy)]
enum ReleaseBump {
    Patch,
    Minor,
    Major,
}

fn bump_from_spec_type(spec_type: &str) -> Option<ReleaseBump> {
    match spec_type {
        "fix" | "refactor" => Some(ReleaseBump::Patch),
        "feat" => Some(ReleaseBump::Minor),
        _ => None,
    }
}

fn compute_next_version(current: &str, bump: ReleaseBump) -> Result<String, String> {
    let parts: Vec<u32> = current
        .split('.')
        .map(|segment| {
            segment
                .parse::<u32>()
                .map_err(|_| format!("Invalid version component '{}'", segment))
        })
        .collect::<Result<Vec<_>, _>>()?;

    if parts.len() != 3 {
        return Err(format!("Expected semver format (x.y.z), got '{}'", current));
    }

    Ok(match bump {
        ReleaseBump::Patch => format!("{}.{}.{}", parts[0], parts[1], parts[2] + 1),
        ReleaseBump::Minor => format!("{}.{}.0", parts[0], parts[1] + 1),
        ReleaseBump::Major => format!("{}.0.0", parts[0] + 1),
    })
}

fn read_cargo_version(root: &Path) -> Result<String, String> {
    let content = fs::read_to_string(root.join("Cargo.toml"))
        .map_err(|e| format!("failed reading Cargo.toml: {}", e))?;
    let mut in_package_section = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_package_section = trimmed == "[package]";
            continue;
        }
        if in_package_section && trimmed.starts_with("version") && trimmed.contains('=') {
            let value = trimmed
                .split('=')
                .nth(1)
                .map(str::trim)
                .map(|v| v.trim_matches('"').trim_matches('\''))
                .filter(|v| !v.is_empty())
                .ok_or_else(|| "Could not parse version in Cargo.toml [package]".to_string())?;
            return Ok(value.to_string());
        }
    }

    Err("Could not find version in Cargo.toml [package]".to_string())
}

fn update_cargo_version(root: &Path, new_version: &str) -> Result<(), String> {
    let path = root.join("Cargo.toml");
    let content =
        fs::read_to_string(&path).map_err(|e| format!("read {}: {}", path.display(), e))?;

    let mut in_package_section = false;
    let mut version_updated = false;
    let mut new_content = String::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_package_section = trimmed == "[package]";
        }
        if in_package_section && !version_updated && trimmed.starts_with("version") {
            new_content.push_str(&format!("version = \"{}\"\n", new_version));
            version_updated = true;
        } else {
            new_content.push_str(line);
            new_content.push('\n');
        }
    }

    if !version_updated {
        return Err("Could not find version field in [package] section of Cargo.toml".to_string());
    }

    fs::write(&path, new_content).map_err(|e| format!("write {}: {}", path.display(), e))
}

fn ensure_release_safeguards(root: &Path, new_version: &str) -> Result<(), String> {
    if !git::is_clean_tracked()? {
        return Err(
            "Working tree has uncommitted changes. Commit or stash before release.".to_string(),
        );
    }

    let behind = git::commits_behind_upstream()?;
    if behind > 0 {
        return Err(format!(
            "Branch is {} commits behind remote. Pull changes first.",
            behind
        ));
    }

    if git::is_diverged()? {
        return Err("Branch has diverged from remote. Resolve divergence first.".to_string());
    }

    let version_tag = format!("v{}", new_version);
    if git::tag_exists(&version_tag)? {
        return Err(format!("Tag '{}' already exists", version_tag));
    }

    let index_path = root.join(".patina/local/data/patina.db");
    if !index_path.exists() {
        return Err(
            "No index found. Run 'patina scrape layer' first to build the index.".to_string(),
        );
    }

    Ok(())
}

fn complete_with_release(
    root: &Path,
    spec: &SpecRecord,
    bump: ReleaseBump,
) -> Result<String, String> {
    let old_version = read_cargo_version(root)?;
    let new_version = compute_next_version(&old_version, bump)?;
    ensure_release_safeguards(root, &new_version)?;

    update_cargo_version(root, &new_version)?;

    let spec_path = Path::new(&spec.path);
    let remove_target = spec_path
        .parent()
        .filter(|parent| parent.file_name().is_some())
        .map(|dir| to_repo_relative(root, dir))
        .unwrap_or_else(|| to_repo_relative(root, spec_path));

    git::remove_paths(std::slice::from_ref(&remove_target))?;

    let mut stage_paths = vec!["Cargo.toml".to_string()];
    if root.join("Cargo.lock").exists() {
        stage_paths.push("Cargo.lock".to_string());
    }
    git::add_paths(&stage_paths)?;

    let title = extract_title(&spec.body)
        .or(spec.frontmatter.title.clone())
        .unwrap_or_else(|| spec.frontmatter.id.clone());
    let commit_msg = format!("release: v{} — {}", new_version, title);
    git::commit(&commit_msg)?;

    let version_tag = format!("v{}", new_version);
    git::create_tag_at(&version_tag, "HEAD")?;

    let spec_tag = format!("spec/{}", spec.frontmatter.id);
    git::create_tag_at(&spec_tag, "HEAD~1")?;

    Ok(new_version)
}

fn extract_frontmatter_and_body(content: &str) -> Option<(&str, &str)> {
    let mut parts = content.splitn(3, "---");
    let first = parts.next()?;
    if !first.trim().is_empty() {
        return None;
    }
    let frontmatter = parts.next()?;
    let body = parts.next().unwrap_or_default();
    Some((frontmatter, body))
}

fn collect_spec_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("read_dir {}: {}", dir.display(), e))?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            collect_spec_files(&path, out)?;
            continue;
        }
        if path.file_name().and_then(|n| n.to_str()) == Some("SPEC.md") {
            out.push(path);
        }
    }
    Ok(())
}

pub(crate) fn load_specs(root: &Path) -> Result<Vec<SpecRecord>, String> {
    let build_root = root.join("layer/surface/build");
    let mut files = Vec::new();
    if !build_root.exists() {
        return Ok(Vec::new());
    }
    collect_spec_files(&build_root, &mut files)?;

    let mut records = Vec::new();
    for file in files {
        let content =
            fs::read_to_string(&file).map_err(|e| format!("read {}: {}", file.display(), e))?;
        let Some((frontmatter_text, body)) = extract_frontmatter_and_body(&content) else {
            continue;
        };
        let frontmatter: SpecFrontmatterLite = serde_yaml::from_str(frontmatter_text)
            .map_err(|e| format!("parse frontmatter {}: {}", file.display(), e))?;
        if frontmatter.id.trim().is_empty() {
            continue;
        }

        let design_path_buf = file.parent().map(|parent| parent.join("DESIGN.md"));
        let (design_path, design_body) = match design_path_buf {
            Some(path) if path.exists() => {
                let body = fs::read_to_string(&path)
                    .map_err(|e| format!("read {}: {}", path.display(), e))?;
                (Some(to_repo_relative(root, &path)), Some(body))
            }
            _ => (None, None),
        };

        records.push(SpecRecord {
            frontmatter,
            path: to_repo_relative(root, &file),
            body: body.to_string(),
            design_path,
            design_body,
        });
    }

    records.sort_by(|a, b| a.frontmatter.id.cmp(&b.frontmatter.id));
    Ok(records)
}

fn normalize_criteria(frontmatter: &SpecFrontmatterLite) -> Vec<(String, String, bool)> {
    frontmatter
        .exit_criteria
        .iter()
        .map(|criterion| match criterion {
            ExitCriterionLite::Text(text) => (slugify(text), text.clone(), false),
            ExitCriterionLite::Full { id, text, checked } => (
                id.clone().unwrap_or_else(|| slugify(text)),
                text.clone(),
                *checked,
            ),
        })
        .collect()
}

fn status_or(frontmatter: &SpecFrontmatterLite, default: &str) -> String {
    frontmatter
        .status
        .clone()
        .unwrap_or_else(|| default.to_string())
}

pub(crate) fn find_spec<'a>(specs: &'a [SpecRecord], id: &str) -> Result<&'a SpecRecord, String> {
    specs
        .iter()
        .find(|record| record.frontmatter.id == id)
        .ok_or_else(|| format!("spec '{}' not found", id))
}

fn is_terminal_status(status: &str) -> bool {
    matches!(status, "complete" | "completed" | "done" | "abandoned")
}

fn archive_spec_record(root: &Path, spec: &SpecRecord, dry_run: bool) -> Result<(), String> {
    let status = status_or(&spec.frontmatter, "unknown");
    if !is_terminal_status(&status) {
        return Err(format!(
            "Spec '{}' has status '{}', expected 'complete' or 'abandoned'",
            spec.frontmatter.id, status
        ));
    }

    let tag_name = format!("spec/{}", spec.frontmatter.id);
    if git::tag_exists(&tag_name)? {
        return Err(format!(
            "Tag '{}' already exists. Spec may have been archived previously.",
            tag_name
        ));
    }

    if dry_run {
        return Ok(());
    }

    if !git::is_clean_tracked()? {
        return Err(
            "Working tree has uncommitted tracked changes. Commit or stash before archiving."
                .to_string(),
        );
    }

    let spec_path = Path::new(&spec.path);
    let remove_target = spec_path
        .parent()
        .filter(|parent| parent.file_name().is_some())
        .map(|dir| to_repo_relative(root, dir))
        .unwrap_or_else(|| to_repo_relative(root, spec_path));
    let spec_path_rel = to_repo_relative(root, spec_path);
    let description = spec
        .frontmatter
        .title
        .clone()
        .unwrap_or_else(|| spec.frontmatter.id.clone());

    git::remove_paths(std::slice::from_ref(&remove_target))?;

    let commit_msg = format!(
        "docs: archive {} ({})\n\nSpec preserved via git tag: {}\nRecover with: git show {}:{}",
        tag_name, status, tag_name, tag_name, spec_path_rel
    );
    git::commit(&commit_msg)?;
    git::create_tag_at(&tag_name, "HEAD~1")?;

    toys::log::info(
        "slate-manager",
        &format!(
            "archived spec id={} status={} target={} description={}",
            spec.frontmatter.id, status, remove_target, description
        ),
    );

    Ok(())
}

pub(crate) fn handle_list(
    root: &Path,
    args: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Result<serde_json::Value, String> {
    let status_filter = arg_string(args, "status");
    let target_filter = arg_string(args, "target");

    let specs = load_specs(root)?;
    let data: Vec<serde_json::Value> = specs
        .into_iter()
        .filter(|spec| {
            let status_ok = status_filter
                .as_deref()
                .is_none_or(|expected| spec.frontmatter.status.as_deref() == Some(expected));
            let target_ok = target_filter
                .as_deref()
                .is_none_or(|expected| spec.frontmatter.target.as_deref() == Some(expected));
            status_ok && target_ok
        })
        .map(|spec| {
            let title = extract_title(&spec.body)
                .or(spec.frontmatter.title.clone())
                .unwrap_or_else(|| spec.frontmatter.id.clone());
            serde_json::json!({
                "id": spec.frontmatter.id,
                "status": spec.frontmatter.status,
                "target": spec.frontmatter.target,
                "title": title,
                "unscraped": true,
            })
        })
        .collect();
    Ok(serde_json::Value::Array(data))
}

fn parse_queue_position(target: Option<&str>) -> Option<u32> {
    target.and_then(|t| t.trim().parse::<u32>().ok())
}

pub(crate) fn handle_next(root: &Path) -> Result<serde_json::Value, String> {
    let specs = load_specs(root)?;

    let mut status_map: HashMap<String, String> = HashMap::new();
    for spec in &specs {
        status_map.insert(
            spec.frontmatter.id.clone(),
            status_or(&spec.frontmatter, "draft"),
        );
    }

    let mut impact_counts: HashMap<String, usize> = HashMap::new();
    for spec in &specs {
        for blocker in &spec.frontmatter.blocked_by {
            *impact_counts.entry(blocker.clone()).or_insert(0) += 1;
        }
    }

    let mut out = Vec::new();

    for spec in specs {
        let status = status_or(&spec.frontmatter, "draft");
        let queue_position = parse_queue_position(spec.frontmatter.target.as_deref());
        let impact = impact_counts
            .get(&spec.frontmatter.id)
            .copied()
            .unwrap_or(0);

        match status.as_str() {
            "active" => {
                out.push(serde_json::json!({
                    "id": spec.frontmatter.id,
                    "status": status,
                    "reason": "Currently active — continue working",
                    "priority": 1,
                    "impact": impact,
                    "queue_position": queue_position,
                }));
            }
            "blocked" => {
                let all_blockers_done = spec.frontmatter.blocked_by.is_empty()
                    || spec.frontmatter.blocked_by.iter().all(|blocker_id| {
                        status_map
                            .get(blocker_id)
                            .map(|value| is_terminal_status(value))
                            .unwrap_or(true)
                    });
                if all_blockers_done {
                    out.push(serde_json::json!({
                        "id": spec.frontmatter.id,
                        "status": status,
                        "reason": "Blockers complete — ready to resume",
                        "priority": 2,
                        "impact": impact,
                        "queue_position": queue_position,
                    }));
                }
            }
            "paused" => {
                out.push(serde_json::json!({
                    "id": spec.frontmatter.id,
                    "status": status,
                    "reason": "Paused",
                    "priority": 4,
                    "impact": impact,
                    "queue_position": queue_position,
                }));
            }
            "ready" => {
                let reason = match queue_position {
                    Some(pos) => format!("Queue position #{}", pos),
                    None if impact > 0 => format!("Ready — blocks {} other spec(s)", impact),
                    None => "Ready to start".to_string(),
                };
                out.push(serde_json::json!({
                    "id": spec.frontmatter.id,
                    "status": status,
                    "reason": reason,
                    "priority": 5,
                    "impact": impact,
                    "queue_position": queue_position,
                }));
            }
            "draft" => {
                let reason = match queue_position {
                    Some(pos) => format!("Queue position #{} — needs audit", pos),
                    None => "Draft — unqueued".to_string(),
                };
                out.push(serde_json::json!({
                    "id": spec.frontmatter.id,
                    "status": status,
                    "reason": reason,
                    "priority": 6,
                    "impact": impact,
                    "queue_position": queue_position,
                }));
            }
            _ => {}
        }
    }

    out.sort_by(|a, b| {
        let ap = a
            .get("priority")
            .and_then(|v| v.as_u64())
            .unwrap_or(u64::MAX);
        let bp = b
            .get("priority")
            .and_then(|v| v.as_u64())
            .unwrap_or(u64::MAX);

        let aq = a.get("queue_position").and_then(|v| v.as_u64());
        let bq = b.get("queue_position").and_then(|v| v.as_u64());

        let ai = a.get("impact").and_then(|v| v.as_u64()).unwrap_or(0);
        let bi = b.get("impact").and_then(|v| v.as_u64()).unwrap_or(0);

        ap.cmp(&bp)
            .then_with(|| match (aq, bq) {
                (Some(la), Some(lb)) => la.cmp(&lb),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            })
            .then_with(|| bi.cmp(&ai))
    });

    Ok(serde_json::Value::Array(out))
}

pub(crate) fn handle_check(
    root: &Path,
    args: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Result<serde_json::Value, String> {
    let id = require_id(args, "check")?;

    let specs = load_specs(root)?;
    let spec = find_spec(&specs, id)?;

    let criteria = normalize_criteria(&spec.frontmatter);
    let total = criteria.len();
    let checked = criteria
        .iter()
        .filter(|(_, _, is_checked)| *is_checked)
        .count();
    let unchecked: Vec<serde_json::Value> = criteria
        .into_iter()
        .filter(|(_, _, is_checked)| !*is_checked)
        .map(|(criterion_id, text, _)| {
            serde_json::json!({
                "id": criterion_id,
                "text": text,
            })
        })
        .collect();

    Ok(serde_json::json!({
        "spec_id": id,
        "total": total,
        "checked": checked,
        "unchecked": unchecked,
        "passed": checked == total,
    }))
}

pub(crate) fn handle_show(
    root: &Path,
    args: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Result<serde_json::Value, String> {
    let id = require_id(args, "show")?;

    let specs = load_specs(root)?;
    let spec = find_spec(&specs, id)?;

    let design_outline = spec.design_body.as_ref().map(|d| extract_outline(d));
    let files = extract_key_files(&spec.body);
    let direct_code_targets = spec
        .design_body
        .as_deref()
        .map(extract_code_targets)
        .unwrap_or_default();
    let resolved_decisions = extract_section_items(&spec.body, "## Resolved Decisions");
    let implementation_order = extract_section_items(&spec.body, "## Implementation Order");
    let verification_points = extract_section_items(&spec.body, "## Verification");
    let open_questions = spec
        .design_body
        .as_deref()
        .map(|d| extract_section_items(d, "## Open Questions"))
        .unwrap_or_default();

    Ok(serde_json::json!({
        "id": spec.frontmatter.id,
        "frontmatter": spec.frontmatter,
        "outline": extract_outline(&spec.body),
        "design_outline": design_outline,
        "files": files,
        "direct_code_targets": direct_code_targets,
        "resolved_decisions": resolved_decisions,
        "implementation_order": implementation_order,
        "verification_points": verification_points,
        "open_questions": open_questions,
        "path": spec.path,
        "design_path": spec.design_path,
    }))
}

pub(crate) fn build_prompt_packet(spec: &SpecRecord) -> serde_json::Value {
    let status = status_or(&spec.frontmatter, "unknown");
    let title = extract_title(&spec.body)
        .or(spec.frontmatter.title.clone())
        .unwrap_or_else(|| spec.frontmatter.id.clone());
    let goal = extract_section_paragraph(&spec.body, "## Goal")
        .unwrap_or_else(|| "Execute this spec in small, verifiable slices.".to_string());
    let direct_code_targets = spec
        .design_body
        .as_deref()
        .map(extract_code_targets)
        .unwrap_or_default();
    let execution_order = extract_section_items(&spec.body, "## Implementation Order");
    let constraints = extract_section_items(&spec.body, "## Non-Goals");
    let verification = extract_section_items(&spec.body, "## Verification");

    let mut definition_of_done: Vec<String> = normalize_criteria(&spec.frontmatter)
        .into_iter()
        .map(|(_, text, _)| format!("- {}", text))
        .collect();
    if definition_of_done.is_empty() {
        definition_of_done
            .push("- Exit criteria are explicitly defined and satisfied.".to_string());
    }

    serde_json::json!({
        "spec_id": spec.frontmatter.id,
        "status": status,
        "title": title,
        "goal": goal,
        "read_first": [
            "layer/core/values/dependable-rust.md",
            "layer/core/values/unix-philosophy.md",
            "layer/core/values/spec-driven-design.md",
            "layer/core/values/safety-boundaries.md"
        ],
        "spec_path": spec.path,
        "design_path": spec.design_path,
        "direct_code_targets": direct_code_targets,
        "execution_order": execution_order,
        "constraints": constraints,
        "verification": verification,
        "definition_of_done": definition_of_done,
        "session_workflow": [
            "Run /session-update periodically.",
            "Run /session-note for important insights.",
            "Run /session-end when complete."
        ]
    })
}

pub(crate) fn build_handoff_packet(spec: &SpecRecord) -> serde_json::Value {
    let status = status_or(&spec.frontmatter, "unknown");
    let title = extract_title(&spec.body)
        .or(spec.frontmatter.title.clone())
        .unwrap_or_else(|| spec.frontmatter.id.clone());

    let criteria = normalize_criteria(&spec.frontmatter);
    let total = criteria.len();
    let checked = criteria
        .iter()
        .filter(|(_, _, is_checked)| *is_checked)
        .count();
    let completed_items: Vec<String> = criteria
        .iter()
        .filter(|(_, _, is_checked)| *is_checked)
        .map(|(_, text, _)| format!("- {}", text))
        .collect();
    let mut open_items: Vec<String> = criteria
        .iter()
        .filter(|(_, _, is_checked)| !*is_checked)
        .map(|(_, text, _)| format!("- {}", text))
        .collect();

    let mut open_questions = spec
        .design_body
        .as_deref()
        .map(|d| extract_section_items(d, "## Open Questions"))
        .unwrap_or_default();
    if open_questions.is_empty() {
        open_questions.push("- No open questions documented.".to_string());
    }
    open_items.extend(open_questions);

    serde_json::json!({
        "spec_id": spec.frontmatter.id,
        "status": status,
        "title": title,
        "progress": {
            "checked": checked,
            "total": total,
        },
        "resolved_decisions": extract_section_items(&spec.body, "## Resolved Decisions"),
        "completed_items": completed_items,
        "open_items": open_items,
        "next_steps": extract_section_items(&spec.body, "## Implementation Order"),
        "verification": extract_section_items(&spec.body, "## Verification"),
        "spec_path": spec.path,
        "design_path": spec.design_path,
    })
}

pub(crate) fn handle_prompt(
    root: &Path,
    args: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Result<serde_json::Value, String> {
    let id = require_id(args, "prompt")?;
    let specs = load_specs(root)?;
    let spec = find_spec(&specs, id)?;
    Ok(build_prompt_packet(spec))
}

pub(crate) fn handle_handoff(
    root: &Path,
    args: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Result<serde_json::Value, String> {
    let id = require_id(args, "handoff")?;
    let specs = load_specs(root)?;
    let spec = find_spec(&specs, id)?;
    Ok(build_handoff_packet(spec))
}

pub(crate) fn handle_packet(
    root: &Path,
    args: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Result<serde_json::Value, String> {
    let id = require_id(args, "packet")?;
    let specs = load_specs(root)?;
    let spec = find_spec(&specs, id)?;
    Ok(serde_json::json!({
        "prompt": build_prompt_packet(spec),
        "handoff": build_handoff_packet(spec),
    }))
}

pub(crate) fn handle_complete(
    root: &Path,
    args: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Result<serde_json::Value, String> {
    let id = require_id(args, "complete")?;
    let force = arg_bool(args, "force", false);
    let major = arg_bool(args, "major", false);

    let specs = load_specs(root)?;
    let spec = find_spec(&specs, id)?;
    let status = status_or(&spec.frontmatter, "unknown");
    if status != "active" {
        return Err(format!(
            "Cannot complete '{}' — status is '{}', expected 'active'",
            id, status
        ));
    }

    let criteria = normalize_criteria(&spec.frontmatter);
    let unchecked: Vec<(String, String)> = criteria
        .iter()
        .filter(|(_, _, checked)| !*checked)
        .map(|(criterion_id, text, _)| (criterion_id.clone(), text.clone()))
        .collect();

    if !unchecked.is_empty() && !force {
        let details = unchecked
            .iter()
            .map(|(criterion_id, text)| format!("  ✗ {} — {}", criterion_id, text))
            .collect::<Vec<_>>()
            .join("\n");
        return Err(format!(
            "Cannot complete '{}' — {} unchecked exit criteria:\n{}\n\n  Use --force to bypass.",
            id,
            unchecked.len(),
            details
        ));
    }

    let spec_type = spec
        .frontmatter
        .r#type
        .clone()
        .unwrap_or_else(|| "explore".to_string());

    let bump = if major {
        Some(ReleaseBump::Major)
    } else {
        bump_from_spec_type(&spec_type)
    };

    if let Some(bump) = bump {
        let new_version = complete_with_release(root, spec, bump)?;
        let _ = new_version;
        return Ok(serde_json::json!({
            "command": "complete",
            "spec_id": id,
            "new_status": "complete",
            "file": spec.path,
            "tag": format!("spec/{}", id),
            "archived": true,
        }));
    }

    let mut completed = spec.clone();
    completed.frontmatter.status = Some("complete".to_string());
    archive_spec_record(root, &completed, false)?;

    Ok(serde_json::json!({
        "command": "complete",
        "spec_id": id,
        "new_status": "complete",
        "file": completed.path,
        "tag": format!("spec/{}", id),
        "archived": true,
    }))
}

pub(crate) fn handle_archive(
    root: &Path,
    args: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Result<serde_json::Value, String> {
    let stale = arg_bool(args, "stale", false);
    let dry_run = arg_bool(args, "dry_run", false);

    let specs = load_specs(root)?;

    if stale {
        let stale_specs: Vec<SpecRecord> = specs
            .into_iter()
            .filter(|spec| is_terminal_status(&status_or(&spec.frontmatter, "unknown")))
            .collect();

        if !dry_run {
            for spec in &stale_specs {
                archive_spec_record(root, spec, false)?;
            }
        }

        return Ok(serde_json::json!({
            "stale": true,
            "dry_run": dry_run,
        }));
    }

    let id = arg_string(args, "id")
        .ok_or_else(|| "Spec ID required. Use `patina spec archive <id>` or --stale".to_string())?;

    let spec = find_spec(&specs, &id)?;
    archive_spec_record(root, spec, dry_run)?;

    Ok(serde_json::json!({
        "id": id,
        "dry_run": dry_run,
    }))
}
