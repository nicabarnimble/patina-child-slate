use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct SpecFrontmatterLite {
    pub(crate) id: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub(crate) r#type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) title: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) blocked_by: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) related: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) beliefs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) references: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) paused_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) blocked_date: Option<String>,
    #[serde(default)]
    pub(crate) exit_criteria: Vec<ExitCriterionLite>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum ExitCriterionLite {
    Text(String),
    Full {
        #[serde(default)]
        id: Option<String>,
        text: String,
        #[serde(default)]
        checked: bool,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct SpecRecord {
    pub(crate) frontmatter: SpecFrontmatterLite,
    pub(crate) path: String,
    pub(crate) body: String,
    pub(crate) design_path: Option<String>,
    pub(crate) design_body: Option<String>,
}

#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum WorkKind {
    #[default]
    Build,
    Fix,
    Refactor,
}

impl WorkKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Fix => "fix",
            Self::Refactor => "refactor",
        }
    }
}

impl fmt::Display for WorkKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for WorkKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "build" | "feat" | "feature" => Ok(Self::Build),
            "fix" | "bug" | "bugfix" => Ok(Self::Fix),
            "refactor" => Ok(Self::Refactor),
            other => Err(format!(
                "invalid Slate work kind '{}': expected build, fix, or refactor",
                other
            )),
        }
    }
}

impl PartialEq<&str> for WorkKind {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

pub(crate) fn normalize_slate_kind(kind: &str) -> WorkKind {
    WorkKind::from_str(kind).unwrap_or(WorkKind::Build)
}

#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum WorkStatus {
    #[default]
    Draft,
    Ready,
    Active,
    Blocked,
    Paused,
    Complete,
    Abandoned,
    Completed,
    Done,
}

impl WorkStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Ready => "ready",
            Self::Active => "active",
            Self::Blocked => "blocked",
            Self::Paused => "paused",
            Self::Complete => "complete",
            Self::Abandoned => "abandoned",
            Self::Completed => "completed",
            Self::Done => "done",
        }
    }

    pub(crate) fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Complete | Self::Abandoned | Self::Completed | Self::Done
        )
    }
}

impl fmt::Display for WorkStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for WorkStatus {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "draft" => Ok(Self::Draft),
            "ready" => Ok(Self::Ready),
            "active" => Ok(Self::Active),
            "blocked" => Ok(Self::Blocked),
            "paused" => Ok(Self::Paused),
            "complete" => Ok(Self::Complete),
            "abandoned" => Ok(Self::Abandoned),
            "completed" => Ok(Self::Completed),
            "done" => Ok(Self::Done),
            other => Err(format!(
                "invalid Slate work status '{}': expected draft, ready, active, blocked, paused, complete, or abandoned",
                other
            )),
        }
    }
}

impl PartialEq<&str> for WorkStatus {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

pub(crate) fn default_slate_status() -> WorkStatus {
    WorkStatus::Draft
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct SlateWorkFile {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) kind: WorkKind,
    #[serde(default = "default_slate_status")]
    pub(crate) status: WorkStatus,
    pub(crate) human_request: String,
    #[serde(default)]
    pub(crate) allium_anchors: Vec<String>,
    #[serde(default)]
    pub(crate) user_alignment: String,
    #[serde(default)]
    pub(crate) belief_refs: Vec<String>,
    #[serde(default)]
    pub(crate) proof_plan: Vec<String>,
    #[serde(default)]
    pub(crate) closure_evidence: Vec<String>,
    #[serde(default)]
    pub(crate) blocked_by: Vec<String>,
    #[serde(default)]
    pub(crate) blocks: Vec<String>,
    #[serde(default)]
    pub(crate) target: Option<String>,
    #[serde(default)]
    pub(crate) implementation_plan: Vec<String>,
    #[serde(default)]
    pub(crate) release_contract: Option<SlateReleaseContract>,
    #[serde(default)]
    pub(crate) belief_harvest_decision: Option<String>,
    #[serde(default)]
    pub(crate) created_at: Option<String>,
    #[serde(default)]
    pub(crate) updated_at: Option<String>,
    #[serde(default)]
    pub(crate) closed_at: Option<String>,
    #[serde(default)]
    pub(crate) block_reason: Option<String>,
    #[serde(default)]
    pub(crate) pause_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub(crate) struct SlateReleaseContract {
    #[serde(default)]
    pub(crate) release_tag: Option<String>,
    #[serde(default)]
    pub(crate) changelog_updated: bool,
    #[serde(default)]
    pub(crate) units: Vec<SlateReleaseUnit>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub(crate) struct SlateReleaseUnit {
    #[serde(default)]
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) ecosystem: String,
    #[serde(default)]
    pub(crate) version_strategy: String,
    #[serde(default)]
    pub(crate) bump_type: Option<String>,
    #[serde(default)]
    pub(crate) version_files: Vec<String>,
    #[serde(default)]
    pub(crate) artifact_build_command: Option<String>,
    #[serde(default)]
    pub(crate) verification: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct SlateWorkRecord {
    pub(crate) work: SlateWorkFile,
    pub(crate) path: String,
    pub(crate) body_path: Option<String>,
    pub(crate) body: String,
}
