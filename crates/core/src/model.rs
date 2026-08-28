use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const RULE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CopyRule {
    pub schema_version: u32,
    pub id: Uuid,
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub source: EndpointConfig,
    pub destination: EndpointConfig,
    #[serde(default)]
    pub filters: FilterConfig,
    #[serde(default)]
    pub copy: CopyOptions,
    #[serde(default)]
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

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "kebab-case")]
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
#[serde(rename_all = "camelCase")]
pub struct FilterConfig {
    #[serde(default)]
    pub exclude: Vec<Exclusion>,
    #[serde(default = "default_true")]
    pub include_hidden: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Exclusion {
    Path { value: String },
    Glob { value: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CopyOptions {
    #[serde(default)]
    pub mode: CopyMode,
    #[serde(default)]
    pub compare: CompareStrategy,
    #[serde(default)]
    pub overwrite: OverwritePolicy,
    #[serde(default)]
    pub symlinks: SymlinkPolicy,
    #[serde(default = "default_true")]
    pub preserve_mtime: bool,
    #[serde(default = "default_concurrency")]
    pub max_concurrency: usize,
    #[serde(default = "default_retries")]
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
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum TriggerConfig {
    #[default]
    Manual,
    Watch {
        #[serde(default = "default_debounce_ms")]
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

const fn default_debounce_ms() -> u64 {
    1000
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
