use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const RULE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CopyRule {
    pub schema_version: u32,
    pub id: Uuid,
    pub name: String,
    pub enabled: bool,
    pub source: EndpointConfig,
    pub destination: EndpointConfig,
    pub filters: FilterConfig,
    pub copy: CopyOptions,
    pub trigger: TriggerConfig,
}

impl CopyRule {
    pub fn validate_structure(&self) -> crate::Result<()> {
        if self.schema_version > RULE_SCHEMA_VERSION {
            return Err(crate::KfcError::UnsupportedSchema {
                actual: self.schema_version,
                supported: RULE_SCHEMA_VERSION,
            });
        }
        if self.schema_version == 0 {
            return Err(crate::KfcError::InvalidRule(
                "schemaVersion must be at least 1".into(),
            ));
        }
        if self.name.trim().is_empty() {
            return Err(crate::KfcError::InvalidRule(
                "name must not be empty".into(),
            ));
        }
        if self.source.path().as_os_str().is_empty()
            || self.destination.path().as_os_str().is_empty()
        {
            return Err(crate::KfcError::InvalidRule(
                "source and destination paths must not be empty".into(),
            ));
        }
        if self.copy.max_concurrency == 0 {
            return Err(crate::KfcError::InvalidRule(
                "maxConcurrency must be greater than zero".into(),
            ));
        }
        if self.copy.max_concurrency > 64 {
            return Err(crate::KfcError::InvalidRule(
                "maxConcurrency must not exceed 64".into(),
            ));
        }
        if self.copy.retry_count > 20 {
            return Err(crate::KfcError::InvalidRule(
                "retryCount must not exceed 20".into(),
            ));
        }
        match &self.trigger {
            TriggerConfig::Watch { debounce_ms } if *debounce_ms < 100 => {
                return Err(crate::KfcError::InvalidRule(
                    "watch debounceMs must be at least 100".into(),
                ));
            }
            TriggerConfig::Interval { seconds } if *seconds == 0 => {
                return Err(crate::KfcError::InvalidRule(
                    "interval seconds must be greater than zero".into(),
                ));
            }
            TriggerConfig::Cron { expression, .. } if expression.trim().is_empty() => {
                return Err(crate::KfcError::InvalidRule(
                    "cron expression must not be empty".into(),
                ));
            }
            _ => {}
        }
        if self
            .filters
            .group
            .as_ref()
            .is_some_and(|group| group.pattern.trim().is_empty())
        {
            return Err(crate::KfcError::InvalidRule(
                "group pattern must not be empty".into(),
            ));
        }
        if self.copy.rename.as_ref().is_some_and(|rename| {
            rename.pattern.trim().is_empty() || rename.destination.trim().is_empty()
        }) {
            return Err(crate::KfcError::InvalidRule(
                "rename pattern and destination must not be empty".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub enum EndpointConfig {
    Local { path: PathBuf },
}

impl EndpointConfig {
    pub fn path(&self) -> &PathBuf {
        match self {
            Self::Local { path } => path,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FilterConfig {
    /// Relative paths selected for copying; each entry includes its subtree.
    #[serde(default)]
    pub include: Vec<String>,
    /// Relative paths deselected as overrides beneath a selected parent.
    #[serde(default)]
    pub exclude: Vec<String>,
    pub include_hidden: bool,
    /// Single version-extraction rule for the group-selection mode.
    #[serde(default)]
    pub group: Option<GroupRule>,
    /// Extracted values (for example versions) the user has checked.
    #[serde(default)]
    pub selected_groups: Vec<String>,
    /// When true, group mode copies only the newest group by version,
    /// ignoring `selected_groups`.
    #[serde(default)]
    pub auto_latest_group: bool,
    /// Which selection model drives the copy: manual or group rules.
    #[serde(default)]
    pub selection_mode: SelectionMode,
}

/// A version-extraction rule used to group top-level file names.
///
/// The regex's first capture group is extracted from each file name and used as
/// the group key; files sharing the same captured value form one group.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GroupRule {
    /// Optional display label shown before the extracted value (for example "Version").
    #[serde(default)]
    pub label: String,
    /// Regex whose first capture group extracts the grouping key (for example the version).
    pub pattern: String,
}

/// Renames destination file names based on the source file name.
///
/// The regex's capture groups are referenced in the destination template as
/// `$1`, `$2`, and so on. Files whose name does not match keep their name.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RenameRule {
    /// Regex with capture groups matched against the source file name.
    pub pattern: String,
    /// Destination name template using `$1..$n` capture references.
    pub destination: String,
}

#[derive(Debug, Clone, Default, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SelectionMode {
    #[default]
    Manual,
    Groups,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CopyOptions {
    pub mode: CopyMode,
    pub compare: CompareStrategy,
    pub overwrite: OverwritePolicy,
    pub symlinks: SymlinkPolicy,
    pub preserve_mtime: bool,
    pub max_concurrency: usize,
    pub retry_count: u32,
    /// Optional rule that renames destination file names from source names.
    #[serde(default)]
    pub rename: Option<RenameRule>,
}

impl Default for CopyOptions {
    fn default() -> Self {
        Self {
            mode: CopyMode::Incremental,
            compare: CompareStrategy::SizeAndMtime,
            overwrite: OverwritePolicy::IfChanged,
            symlinks: SymlinkPolicy::Skip,
            preserve_mtime: true,
            max_concurrency: default_concurrency(),
            retry_count: default_retries(),
            rename: None,
        }
    }
}

const fn default_concurrency() -> usize {
    4
}
const fn default_retries() -> u32 {
    2
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CopyMode {
    #[default]
    Incremental,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CompareStrategy {
    #[default]
    SizeAndMtime,
    SizeOnly,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum OverwritePolicy {
    #[default]
    IfChanged,
    Never,
    Always,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SymlinkPolicy {
    #[default]
    Skip,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub enum TriggerConfig {
    #[default]
    Manual,
    Watch {
        debounce_ms: u64,
    },
    Interval {
        seconds: u64,
    },
    Cron {
        expression: String,
        #[serde(default)]
        timezone: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CopyPlan {
    pub rule_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub source_root: PathBuf,
    pub destination_root: PathBuf,
    pub items: Vec<PlanItem>,
    pub stats: PlanStats,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlanItem {
    pub action: PlanAction,
    pub relative_path: PathBuf,
    /// Destination relative path when the file is renamed; otherwise the source path.
    #[serde(default)]
    pub destination_path: Option<PathBuf>,
    /// Warning attached to an item that still performs its action.
    #[serde(default)]
    pub warning: Option<String>,
    pub size: u64,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PlanAction {
    CreateDirectory,
    CopyFile,
    Skip,
    Warn,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlanStats {
    pub directories_to_create: u64,
    pub files_to_copy: u64,
    pub files_to_skip: u64,
    pub warnings: u64,
    pub bytes_to_copy: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PlanProgressPhase {
    Scanning,
    Planning,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlanProgressEvent {
    pub phase: PlanProgressPhase,
    pub completed_entries: u64,
    pub total_entries: Option<u64>,
    pub relative_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct JobSummary {
    pub job_id: Uuid,
    pub rule_id: Uuid,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub status: JobStatus,
    pub copied_files: u64,
    pub skipped_files: u64,
    pub failed_files: u64,
    pub copied_bytes: u64,
    pub errors: Vec<FileError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum JobStatus {
    Succeeded,
    Partial,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileError {
    pub relative_path: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEvent {
    pub job_id: Uuid,
    pub relative_path: PathBuf,
    pub completed_files: u64,
    pub total_files: u64,
    pub copied_bytes: u64,
    pub total_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_rule() -> CopyRule {
        CopyRule {
            schema_version: RULE_SCHEMA_VERSION,
            id: Uuid::new_v4(),
            name: "Schema test".into(),
            enabled: true,
            source: EndpointConfig::Local {
                path: "source".into(),
            },
            destination: EndpointConfig::Local {
                path: "destination".into(),
            },
            filters: FilterConfig::default(),
            copy: CopyOptions::default(),
            trigger: TriggerConfig::Manual,
        }
    }

    #[test]
    fn v1_deserialization_requires_all_schema_fields() {
        let mut value = serde_json::to_value(test_rule()).unwrap();
        value.as_object_mut().unwrap().remove("filters");
        assert!(serde_json::from_value::<CopyRule>(value).is_err());
    }

    #[test]
    fn v1_deserialization_rejects_unknown_fields() {
        let mut value = serde_json::to_value(test_rule()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("unexpected".into(), serde_json::Value::Bool(true));
        assert!(serde_json::from_value::<CopyRule>(value).is_err());
    }

    #[test]
    fn v1_deserialization_defaults_missing_include_overrides() {
        let mut value = serde_json::to_value(test_rule()).unwrap();
        value["filters"].as_object_mut().unwrap().remove("include");
        let rule = serde_json::from_value::<CopyRule>(value).unwrap();
        assert!(rule.filters.include.is_empty());
    }

    #[test]
    fn retry_count_matches_schema_limit() {
        let mut rule = test_rule();
        rule.copy.retry_count = 21;
        assert!(matches!(
            rule.validate_structure(),
            Err(crate::KfcError::InvalidRule(_))
        ));
    }

    #[test]
    fn plan_progress_serializes_for_the_frontend_contract() {
        let value = serde_json::to_value(PlanProgressEvent {
            phase: PlanProgressPhase::Planning,
            completed_entries: 7,
            total_entries: Some(10),
            relative_path: Some("nested/file.txt".into()),
        })
        .unwrap();

        assert_eq!(value["phase"], "planning");
        assert_eq!(value["completedEntries"], 7);
        assert_eq!(value["totalEntries"], 10);
        assert_eq!(value["relativePath"], "nested/file.txt");
    }
}
