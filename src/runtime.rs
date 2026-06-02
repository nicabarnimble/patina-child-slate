use std::path::{Path, PathBuf};

pub(crate) fn extract_command_name(payload: &serde_json::Value) -> Option<String> {
    let command = payload.get("command")?.as_object()?;
    let key = command.keys().next()?.to_ascii_lowercase();
    Some(key)
}

pub(crate) fn extract_backend_mode(payload: &serde_json::Value) -> String {
    payload
        .get("backend_mode")
        .and_then(|value| value.as_str())
        .map(|value| value.to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "off".to_string())
}

pub(crate) fn extract_command_args(
    payload: &serde_json::Value,
) -> Option<&serde_json::Map<String, serde_json::Value>> {
    let command = payload.get("command")?.as_object()?;
    let variant = command.values().next()?;
    variant.as_object()
}

pub(crate) fn is_patina_project_root(path: &Path) -> bool {
    path.join(".patina").is_dir() && path.join("layer").is_dir()
}

pub(crate) fn find_project_root() -> Result<PathBuf, String> {
    let mut current = std::env::current_dir().map_err(|e| e.to_string())?;
    loop {
        if is_patina_project_root(&current) {
            return Ok(current);
        }
        let Some(parent) = current.parent() else {
            return Err("not in a Patina project".to_string());
        };
        current = parent.to_path_buf();
    }
}

pub(crate) fn resolve_project_root_from_hint(project: Option<&str>) -> Result<PathBuf, String> {
    if let Some(project) = project {
        let trimmed = project.trim();
        if !trimmed.is_empty() {
            let candidate = PathBuf::from(trimmed);
            let resolved = if candidate.is_absolute() {
                candidate
            } else {
                std::env::current_dir()
                    .map_err(|e| e.to_string())?
                    .join(candidate)
            };
            if is_patina_project_root(&resolved) {
                return Ok(resolved);
            }

            return Err(format!(
                "invalid project root in slate envelope: {}; Patina/Mother must mount the host project at /project and pass the guest project path",
                resolved.display()
            ));
        }
    }

    find_project_root()
}

pub(crate) fn resolve_project_root_from_envelope(
    envelope: &serde_json::Value,
) -> Result<PathBuf, String> {
    resolve_project_root_from_hint(envelope.get("project").and_then(|value| value.as_str()))
}

pub(crate) fn with_project_root_cwd<T>(
    project_root: &Path,
    f: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let original = std::env::current_dir().ok();
    std::env::set_current_dir(project_root).map_err(|e| {
        format!(
            "failed to enter project root {}: {}",
            project_root.display(),
            e
        )
    })?;

    let result = f();

    if let Some(original) = original {
        let _ = std::env::set_current_dir(&original);
    }

    result
}

pub(crate) fn require_id<'a>(
    args: Option<&'a serde_json::Map<String, serde_json::Value>>,
    command: &str,
) -> Result<&'a str, String> {
    args.and_then(|map| map.get("id"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("{} requires id", command))
}

pub(crate) fn arg_bool(
    args: Option<&serde_json::Map<String, serde_json::Value>>,
    key: &str,
    default: bool,
) -> bool {
    args.and_then(|map| map.get(key))
        .and_then(|v| v.as_bool())
        .unwrap_or(default)
}

pub(crate) fn arg_string(
    args: Option<&serde_json::Map<String, serde_json::Value>>,
    key: &str,
) -> Option<String> {
    args.and_then(|map| map.get(key))
        .and_then(|v| v.as_str())
        .map(|v| v.to_string())
}

pub(crate) fn to_repo_relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}
