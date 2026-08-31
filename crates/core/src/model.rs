use std::{collections::HashSet, path::PathBuf};

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
        if self.copy.mode == CopyMode::Versioned {
            self.validate_versioned_copy()?;
        }
        Ok(())
    }

    fn validate_versioned_copy(&self) -> crate::Result<()> {
        let versioned = &self.copy.versioned;
        if let Some(pattern) = &versioned.version_pattern
            && !pattern.trim().is_empty()
        {
            regex::Regex::new(pattern).map_err(|error| crate::KfcError::InvalidPattern {
                pattern: pattern.clone(),
                message: error.to_string(),
            })?;
        }
        if let Some(pattern) = &versioned.rename_pattern
            && !pattern.trim().is_empty()
        {
            regex::Regex::new(pattern).map_err(|error| crate::KfcError::InvalidPattern {
                pattern: pattern.clone(),
                message: error.to_string(),
            })?;
        }
        if let Some(template) = &versioned.target_name_template
            && !template.trim().is_empty()
        {
            validate_target_template(template)?;
        }
        if versioned.mappings.is_empty() {
            return Ok(());
        }
        let mut target_names = HashSet::new();
        for mapping in &versioned.mappings {
            if mapping.source_pattern.trim().is_empty() {
                return Err(crate::KfcError::InvalidRule(
                    "versioned mapping sourcePattern must not be empty".into(),
                ));
            }
            globset::Glob::new(&mapping.source_pattern.replace('\\', "/")).map_err(|error| {
                crate::KfcError::InvalidPattern {
                    pattern: mapping.source_pattern.clone(),
                    message: error.to_string(),
                }
            })?;
            if mapping.target_name.trim().is_empty() {
                return Err(crate::KfcError::InvalidRule(
                    "versioned mapping targetName must not be empty".into(),
                ));
            }
            let target = crate::normalize_target_name(&mapping.target_name)?;
            if !target_names.insert(target.clone()) {
                return Err(crate::KfcError::InvalidRule(format!(
                    "versioned mappings produce duplicate target file '{target}'"
                )));
            }
            if mapping.selection == VersionSelection::Exact {
                let version = mapping.version.as_deref().ok_or_else(|| {
                    crate::KfcError::InvalidRule(format!(
                        "versioned mapping '{target}' selects an exact version but version is not set"
                    ))
                })?;
                if crate::parse_version(version).is_none() {
                    return Err(crate::KfcError::InvalidRule(format!(
                        "versioned mapping '{target}' has an unparsable exact version '{version}'"
                    )));
                }
            }
            if let Some(pattern) = &mapping.version_pattern {
                regex::Regex::new(pattern).map_err(|error| crate::KfcError::InvalidPattern {
                    pattern: pattern.clone(),
                    message: error.to_string(),
                })?;
            }
        }
        Ok(())
    }
}

fn validate_target_template(template: &str) -> crate::Result<()> {
    let mut remaining = template;
    while let Some(open) = remaining.find('{') {
        let Some(close) = remaining[open..].find('}') else {
            return Err(crate::KfcError::InvalidRule(format!(
                "target name template has an unclosed placeholder: '{template}'"
            )));
        };
        let name = &remaining[open + 1..open + close];
        let is_capture_group = !name.is_empty() && name.chars().all(|c| c.is_ascii_digit());
        if !matches!(name, "name" | "version" | "ext") && !is_capture_group {
            return Err(crate::KfcError::InvalidRule(format!(
                "unknown target name placeholder '{{{name}}}' in template '{template}'"
            )));
        }
        remaining = &remaining[open + close + 1..];
    }
    Ok(())
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
    #[serde(default)]
    pub include: Vec<String>,
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
    #[serde(default)]
    pub versioned: VersionedCopyOptions,
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
            versioned: VersionedCopyOptions::default(),
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
    Versioned,
}

/// Configuration used when [`CopyMode::Versioned`] is selected. The source
/// folder may contain several releases of the same binary; each mapping picks
/// one matching file and copies it to a fixed destination file name.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VersionedCopyOptions {
    /// Explicit mappings. When empty, mappings are auto-derived from the source
    /// by grouping versioned files by base name (version stripped).
    #[serde(default)]
    pub mappings: Vec<VersionedMapping>,
    /// Optional rename template used for auto-derived target names. Supports
    /// `{name}`, `{version}`, and `{ext}` placeholders; defaults to `{name}.{ext}`.
    #[serde(default)]
    pub target_name_template: Option<String>,
    /// Optional custom regular expression used to extract the version from a
    /// file name during auto-detection. Mappings may override it individually.
    #[serde(default)]
    pub version_pattern: Option<String>,
    /// Optional regular expression applied to the chosen source file name whose
    /// capture groups fill `{1}`, `{2}`, ... placeholders in target names.
    #[serde(default)]
    pub rename_pattern: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VersionedMapping {
    /// Glob matched against source file names, e.g. `EDM.DSP.Core*.dll`.
    pub source_pattern: String,
    /// Fixed destination file name, e.g. `EDM.DSP.Core.dll`. May contain
    /// nested relative directories such as `bin/EDM.DSP.Core.dll`.
    pub target_name: String,
    /// Whether to pick the newest version or a specific version.
    pub selection: VersionSelection,
    /// Required version when `selection` is `exact`.
    #[serde(default)]
    pub version: Option<String>,
    /// Optional custom regular expression used to extract the version from a
    /// file name. Defaults to the first dotted-numeric token.
    #[serde(default)]
    pub version_pattern: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum VersionSelection {
    #[default]
    Latest,
    Exact,
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
    /// Destination-relative path (also the source-relative path when
    /// `source_relative_path` is absent).
    pub relative_path: PathBuf,
    /// Source-relative path when it differs from the destination path, such
    /// as a versioned source file copied to a fixed destination name.
    #[serde(default)]
    pub source_relative_path: Option<PathBuf>,
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
    fn v1_deserialization_defaults_missing_versioned_config() {
        let mut value = serde_json::to_value(test_rule()).unwrap();
        value["copy"].as_object_mut().unwrap().remove("versioned");
        let rule = serde_json::from_value::<CopyRule>(value).unwrap();
        assert_eq!(rule.copy.mode, CopyMode::Incremental);
        assert!(rule.copy.versioned.mappings.is_empty());
    }

    #[test]
    fn versioned_mode_allows_auto_derived_mappings() {
        let mut rule = test_rule();
        rule.copy.mode = CopyMode::Versioned;
        rule.copy.versioned = VersionedCopyOptions::default();
        rule.validate_structure().unwrap();
    }

    #[test]
    fn target_name_template_rejects_unknown_placeholders() {
        let mut rule = test_rule();
        rule.copy.mode = CopyMode::Versioned;
        rule.copy.versioned = VersionedCopyOptions {
            mappings: vec![],
            target_name_template: Some("EDM.{artifact}.dll".into()),
            version_pattern: None,
            rename_pattern: None,
        };
        assert!(matches!(
            rule.validate_structure(),
            Err(crate::KfcError::InvalidRule(_))
        ));

        rule.copy.versioned.target_name_template = Some("EDM.BIN0003{name}.dll".into());
        rule.validate_structure().unwrap();
    }

    #[test]
    fn versioned_exact_selection_requires_a_parseable_version() {
        let mut rule = test_rule();
        rule.copy.mode = CopyMode::Versioned;
        rule.copy.versioned = VersionedCopyOptions {
            mappings: vec![VersionedMapping {
                source_pattern: "EDM.DSP.Core*.dll".into(),
                target_name: "EDM.DSP.Core.dll".into(),
                selection: VersionSelection::Exact,
                version: None,
                version_pattern: None,
            }],
            target_name_template: None,
            version_pattern: None,
            rename_pattern: None,
        };
        assert!(matches!(
            rule.validate_structure(),
            Err(crate::KfcError::InvalidRule(_))
        ));

        rule.copy.versioned.mappings[0].version = Some("not-a-version".into());
        assert!(matches!(
            rule.validate_structure(),
            Err(crate::KfcError::InvalidRule(_))
        ));
    }

    #[test]
    fn versioned_rule_round_trips_with_its_mappings() {
        let mut rule = test_rule();
        rule.copy.mode = CopyMode::Versioned;
        rule.copy.versioned = VersionedCopyOptions {
            mappings: vec![
                VersionedMapping {
                    source_pattern: "EDM.DSP.Core*.dll".into(),
                    target_name: "EDM.DSP.Core.dll".into(),
                    selection: VersionSelection::Latest,
                    version: None,
                    version_pattern: None,
                },
                VersionedMapping {
                    source_pattern: "EDM.DSP.Proxy*.dll".into(),
                    target_name: "bin/EDM.DSP.Proxy.dll".into(),
                    selection: VersionSelection::Exact,
                    version: Some("1.2.0".into()),
                    version_pattern: Some(r"v(\d+\.\d+\.\d+)".into()),
                },
            ],
            target_name_template: Some("EDM.BIN0003{name}.dll".into()),
            version_pattern: None,
            rename_pattern: Some(r"^80([A-Za-z]+)_".into()),
        };
        rule.validate_structure().unwrap();

        let round_tripped: CopyRule =
            serde_json::from_value(serde_json::to_value(&rule).unwrap()).unwrap();
        assert_eq!(round_tripped, rule);
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
