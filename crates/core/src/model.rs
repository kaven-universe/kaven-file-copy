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
    pub exclude: Vec<Exclusion>,
    pub include_hidden: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Exclusion {
    Path { value: String },
    Glob { value: String },
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
    fn retry_count_matches_schema_limit() {
        let mut rule = test_rule();
        rule.copy.retry_count = 21;
        assert!(matches!(
            rule.validate_structure(),
            Err(crate::KfcError::InvalidRule(_))
        ));
    }
}
