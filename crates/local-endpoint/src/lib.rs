mod endpoint;

pub use endpoint::LocalEndpoint;

use std::{
    collections::{BTreeMap, HashSet, VecDeque},
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::SystemTime,
};

use chrono::Utc;
use filetime::FileTime;
use globset::Glob;
use kfc_core::{
    CompareStrategy, CopyMode, CopyPlan, CopyRule, ExclusionMatcher, FileError, IoResultExt,
    JobStatus, JobSummary, KfcError, OverwritePolicy, PlanAction, PlanItem, PlanProgressEvent,
    PlanProgressPhase, PlanStats, ProgressEvent, Result, VersionSelection, VersionedCopyOptions,
    VersionedMapping, base_name_of, capture_groups, escape_glob, extract_version, file_extension,
    normalize_target_name, parse_version, render_target_template,
};
use tracing::{debug, error, info, warn};
use uuid::Uuid;
use walkdir::WalkDir;

#[derive(Debug, Clone, Default)]
pub struct LocalCopyEngine;

impl LocalCopyEngine {
    pub fn validate_rule(&self, rule: &CopyRule) -> Result<ValidatedPaths> {
        rule.validate_structure()?;
        ExclusionMatcher::new(&rule.filters.exclude, &rule.filters.include)?;

        let source = rule.source.path();
        let destination = rule.destination.path();
        let source_metadata = fs::metadata(source).at(source)?;
        let destination_metadata = fs::metadata(destination).at(destination)?;
        if !destination_metadata.is_dir() {
            return Err(KfcError::InvalidRule(
                "destination must be a directory".into(),
            ));
        }

        let source = fs::canonicalize(source).at(source)?;
        let destination = fs::canonicalize(destination).at(destination)?;
        if source == destination {
            return Err(KfcError::UnsafeRelationship(
                "source and destination resolve to the same path".into(),
            ));
        }
        if source_metadata.is_dir() && destination.starts_with(&source) {
            return Err(KfcError::UnsafeRelationship(
                "destination cannot be located inside the source directory".into(),
            ));
        }
        Ok(ValidatedPaths {
            source,
            destination,
            source_is_file: source_metadata.is_file(),
        })
    }

    pub fn plan(&self, rule: &CopyRule) -> Result<CopyPlan> {
        self.plan_internal(rule, None::<fn(&PlanProgressEvent)>)
    }

    pub fn plan_with_progress<F>(&self, rule: &CopyRule, progress: F) -> Result<CopyPlan>
    where
        F: FnMut(&PlanProgressEvent),
    {
        self.plan_internal(rule, Some(progress))
    }

    fn plan_internal<F>(&self, rule: &CopyRule, mut progress: Option<F>) -> Result<CopyPlan>
    where
        F: FnMut(&PlanProgressEvent),
    {
        let paths = self.validate_rule(rule)?;
        if rule.copy.mode == CopyMode::Versioned {
            return self.plan_versioned_internal(rule, &paths, &mut progress);
        }
        let matcher = ExclusionMatcher::new(&rule.filters.exclude, &rule.filters.include)?;
        let mut items = Vec::new();
        let mut stats = PlanStats::default();
        let plan_source_root;
        emit_plan_progress(
            &mut progress,
            PlanProgressEvent {
                phase: PlanProgressPhase::Scanning,
                completed_entries: 0,
                total_entries: None,
                relative_path: None,
            },
        );

        if paths.source_is_file {
            let name = paths.source.file_name().ok_or_else(|| {
                KfcError::InvalidRule("source file does not have a file name".into())
            })?;
            plan_source_root = paths
                .source
                .parent()
                .ok_or_else(|| KfcError::PathEscape(paths.source.display().to_string()))?
                .to_owned();
            let relative = Path::new(name);
            emit_plan_progress(
                &mut progress,
                PlanProgressEvent {
                    phase: PlanProgressPhase::Scanning,
                    completed_entries: 1,
                    total_entries: None,
                    relative_path: Some(relative.to_owned()),
                },
            );
            emit_plan_progress(
                &mut progress,
                PlanProgressEvent {
                    phase: PlanProgressPhase::Planning,
                    completed_entries: 0,
                    total_entries: Some(1),
                    relative_path: None,
                },
            );
            if !matcher.is_excluded(relative)
                && (rule.filters.include_hidden || !is_hidden(relative))
            {
                self.plan_file(
                    rule,
                    PlannedSource {
                        absolute: &paths.source,
                        relative: None,
                    },
                    relative,
                    &paths.destination,
                    &mut items,
                    &mut stats,
                )?;
            }
            emit_plan_progress(
                &mut progress,
                PlanProgressEvent {
                    phase: PlanProgressPhase::Planning,
                    completed_entries: 1,
                    total_entries: Some(1),
                    relative_path: Some(relative.to_owned()),
                },
            );
        } else {
            plan_source_root = paths.source.clone();
            let total_entries = if progress.is_some() {
                let mut total_entries = 0;
                for entry in WalkDir::new(&paths.source)
                    .follow_links(false)
                    .into_iter()
                    .filter_entry(|entry| should_visit(entry, &paths.source, rule, &matcher))
                {
                    let entry = entry.map_err(|error| {
                        KfcError::InvalidRule(format!("source scan failed: {error}"))
                    })?;
                    if entry.path() == paths.source {
                        continue;
                    }
                    let relative = entry
                        .path()
                        .strip_prefix(&paths.source)
                        .map_err(|_| KfcError::PathEscape(entry.path().display().to_string()))?;
                    total_entries += 1;
                    emit_plan_progress(
                        &mut progress,
                        PlanProgressEvent {
                            phase: PlanProgressPhase::Scanning,
                            completed_entries: total_entries,
                            total_entries: None,
                            relative_path: Some(relative.to_owned()),
                        },
                    );
                }
                emit_plan_progress(
                    &mut progress,
                    PlanProgressEvent {
                        phase: PlanProgressPhase::Planning,
                        completed_entries: 0,
                        total_entries: Some(total_entries),
                        relative_path: None,
                    },
                );
                Some(total_entries)
            } else {
                None
            };

            let mut completed_entries = 0;
            for entry in WalkDir::new(&paths.source)
                .follow_links(false)
                .into_iter()
                .filter_entry(|entry| should_visit(entry, &paths.source, rule, &matcher))
            {
                let entry = entry.map_err(|error| {
                    KfcError::InvalidRule(format!("source scan failed: {error}"))
                })?;
                if entry.path() == paths.source {
                    continue;
                }
                let relative = entry
                    .path()
                    .strip_prefix(&paths.source)
                    .map_err(|_| KfcError::PathEscape(entry.path().display().to_string()))?;
                if matcher.is_excluded(relative) {
                    report_completed_entry(
                        &mut progress,
                        &mut completed_entries,
                        total_entries,
                        relative,
                    );
                    continue;
                }
                let file_type = entry.file_type();
                if file_type.is_symlink() {
                    stats.warnings += 1;
                    items.push(PlanItem {
                        action: PlanAction::Warn,
                        relative_path: relative.to_owned(),
                        source_relative_path: None,
                        size: 0,
                        reason: "symbolic link skipped by policy".into(),
                    });
                } else if file_type.is_dir() {
                    let target = paths.destination.join(relative);
                    if !target.exists() {
                        stats.directories_to_create += 1;
                        items.push(PlanItem {
                            action: PlanAction::CreateDirectory,
                            relative_path: relative.to_owned(),
                            source_relative_path: None,
                            size: 0,
                            reason: "destination directory does not exist".into(),
                        });
                    }
                } else if file_type.is_file() {
                    self.plan_file(
                        rule,
                        PlannedSource {
                            absolute: entry.path(),
                            relative: None,
                        },
                        relative,
                        &paths.destination,
                        &mut items,
                        &mut stats,
                    )?;
                }
                report_completed_entry(
                    &mut progress,
                    &mut completed_entries,
                    total_entries,
                    relative,
                );
            }
        }

        Ok(CopyPlan {
            rule_id: rule.id,
            created_at: Utc::now(),
            source_root: plan_source_root,
            destination_root: paths.destination,
            items,
            stats,
        })
    }

    /// Plans a `versioned` copy: scans the source tree once, then for every
    /// configured mapping picks one matching file (newest version, an exact
    /// version, or the single match) and maps it to a fixed destination name.
    fn plan_versioned_internal<F>(
        &self,
        rule: &CopyRule,
        paths: &ValidatedPaths,
        progress: &mut Option<F>,
    ) -> Result<CopyPlan>
    where
        F: FnMut(&PlanProgressEvent),
    {
        let matcher = ExclusionMatcher::new(&rule.filters.exclude, &rule.filters.include)?;
        let total_entries = if progress.is_some() {
            let mut total = 0;
            for entry in WalkDir::new(&paths.source)
                .follow_links(false)
                .into_iter()
                .filter_entry(|entry| should_visit(entry, &paths.source, rule, &matcher))
            {
                let entry = entry.map_err(|error| {
                    KfcError::InvalidRule(format!("source scan failed: {error}"))
                })?;
                if entry.path() != paths.source {
                    total += 1;
                }
            }
            Some(total)
        } else {
            None
        };

        let mut candidates: Vec<(PathBuf, String)> = Vec::new();
        let mut completed_entries = 0;
        for entry in WalkDir::new(&paths.source)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| should_visit(entry, &paths.source, rule, &matcher))
        {
            let entry = entry
                .map_err(|error| KfcError::InvalidRule(format!("source scan failed: {error}")))?;
            if entry.path() == paths.source {
                continue;
            }
            let relative = entry
                .path()
                .strip_prefix(&paths.source)
                .map_err(|_| KfcError::PathEscape(entry.path().display().to_string()))?;
            if entry.file_type().is_file() {
                candidates.push((
                    relative.to_owned(),
                    entry.file_name().to_string_lossy().into_owned(),
                ));
            }
            report_completed_entry(progress, &mut completed_entries, total_entries, relative);
        }

        let mut items = Vec::new();
        let mut stats = PlanStats::default();
        let effective_mappings = if rule.copy.versioned.mappings.is_empty() {
            build_auto_mappings(&candidates, &rule.copy.versioned)?
        } else {
            rule.copy.versioned.mappings.clone()
        };
        let mut seen_targets = HashSet::new();
        for mapping in &effective_mappings {
            let pattern = mapping
                .version_pattern
                .as_deref()
                .filter(|pattern| !pattern.trim().is_empty())
                .or_else(|| {
                    rule.copy
                        .versioned
                        .version_pattern
                        .as_deref()
                        .filter(|pattern| !pattern.trim().is_empty())
                });
            let glob = Glob::new(&mapping.source_pattern.replace('\\', "/"))
                .map_err(|error| KfcError::InvalidPattern {
                    pattern: mapping.source_pattern.clone(),
                    message: error.to_string(),
                })?
                .compile_matcher();
            let matched = candidates
                .iter()
                .filter(|(_, name)| glob.is_match(name))
                .collect::<Vec<_>>();
            let Some(chosen) = select_versioned_candidate(
                &matched,
                &mapping.selection,
                mapping.version.as_deref(),
                pattern,
            )?
            else {
                stats.warnings += 1;
                let target_relative = PathBuf::from(normalize_target_name(&mapping.target_name)?);
                items.push(PlanItem {
                    action: PlanAction::Warn,
                    relative_path: target_relative,
                    source_relative_path: None,
                    size: 0,
                    reason: format!(
                        "no source file matches pattern '{}'",
                        mapping.source_pattern
                    ),
                });
                continue;
            };
            let target_relative =
                render_mapping_target(mapping, &chosen.1, pattern, &rule.copy.versioned)?;
            if !seen_targets.insert(target_relative.clone()) {
                return Err(KfcError::InvalidRule(format!(
                    "versioned mappings produce duplicate target file '{}'",
                    target_relative.display()
                )));
            }
            let source_absolute = paths.source.join(&chosen.0);
            self.plan_file(
                rule,
                PlannedSource {
                    absolute: &source_absolute,
                    relative: Some(&chosen.0),
                },
                &target_relative,
                &paths.destination,
                &mut items,
                &mut stats,
            )?;
        }

        Ok(CopyPlan {
            rule_id: rule.id,
            created_at: Utc::now(),
            source_root: paths.source.clone(),
            destination_root: paths.destination.clone(),
            items,
            stats,
        })
    }

    /// Scans the source and auto-derives a suggested mapping list from the
    /// versioned file families found there, using the rule-level version
    /// pattern and target-name template. Used by the UI "Detect mappings"
    /// button so users can review and adjust the derived mappings.
    pub fn detect_versioned_mappings(&self, rule: &CopyRule) -> Result<Vec<VersionedMapping>> {
        rule.validate_structure()?;
        let paths = self.validate_rule(rule)?;
        let matcher = ExclusionMatcher::new(&rule.filters.exclude, &rule.filters.include)?;
        let mut candidates = Vec::new();
        for entry in WalkDir::new(&paths.source)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| should_visit(entry, &paths.source, rule, &matcher))
        {
            let entry = entry
                .map_err(|error| KfcError::InvalidRule(format!("source scan failed: {error}")))?;
            if entry.path() == paths.source {
                continue;
            }
            if entry.file_type().is_file() {
                let relative = entry
                    .path()
                    .strip_prefix(&paths.source)
                    .map_err(|_| KfcError::PathEscape(entry.path().display().to_string()))?;
                candidates.push((
                    relative.to_owned(),
                    entry.file_name().to_string_lossy().into_owned(),
                ));
            }
        }
        build_auto_mappings(&candidates, &rule.copy.versioned)
    }

    fn plan_file(
        &self,
        rule: &CopyRule,
        source: PlannedSource<'_>,
        relative: &Path,
        destination_root: &Path,
        items: &mut Vec<PlanItem>,
        stats: &mut PlanStats,
    ) -> Result<()> {
        let source_metadata = fs::metadata(source.absolute).at(source.absolute)?;
        let target = destination_root.join(relative);
        let (action, reason) = match fs::metadata(&target) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (
                PlanAction::CopyFile,
                "destination file does not exist".into(),
            ),
            Err(error) => {
                return Err(KfcError::Io {
                    path: target,
                    source: error,
                });
            }
            Ok(target_metadata) if !target_metadata.is_file() => (
                PlanAction::Warn,
                "destination path exists but is not a file".into(),
            ),
            Ok(target_metadata) => {
                let changed = match rule.copy.compare {
                    CompareStrategy::SizeOnly => source_metadata.len() != target_metadata.len(),
                    CompareStrategy::SizeAndMtime => {
                        source_metadata.len() != target_metadata.len()
                            || modified(&source_metadata) != modified(&target_metadata)
                    }
                };
                match rule.copy.overwrite {
                    OverwritePolicy::Never => {
                        (PlanAction::Skip, "overwrite policy is never".into())
                    }
                    OverwritePolicy::Always => {
                        (PlanAction::CopyFile, "overwrite policy is always".into())
                    }
                    OverwritePolicy::IfChanged if changed => (
                        PlanAction::CopyFile,
                        "source differs from destination".into(),
                    ),
                    OverwritePolicy::IfChanged => (
                        PlanAction::Skip,
                        "source and destination metadata match".into(),
                    ),
                }
            }
        };

        match action {
            PlanAction::CopyFile => {
                stats.files_to_copy += 1;
                stats.bytes_to_copy += source_metadata.len();
            }
            PlanAction::Skip => stats.files_to_skip += 1,
            PlanAction::Warn => stats.warnings += 1,
            PlanAction::CreateDirectory => {}
        }
        items.push(PlanItem {
            action,
            relative_path: relative.to_owned(),
            source_relative_path: source.relative.map(Path::to_owned),
            size: source_metadata.len(),
            reason,
        });
        Ok(())
    }

    pub fn execute<F>(
        &self,
        rule: &CopyRule,
        plan: &CopyPlan,
        cancelled: &AtomicBool,
        mut progress: F,
    ) -> Result<JobSummary>
    where
        F: FnMut(&ProgressEvent),
    {
        let job_id = Uuid::new_v4();
        let started_at = Utc::now();
        let mut copied_files = 0;
        let mut skipped_files = 0;
        let mut copied_bytes = 0;
        let mut errors = Vec::new();
        let total_files = plan.stats.files_to_copy;
        let mut processed_files = 0;

        info!(%job_id, rule_id = %rule.id, files = total_files, bytes = plan.stats.bytes_to_copy, "copy job started");

        let mut files = VecDeque::new();
        for item in &plan.items {
            match item.action {
                PlanAction::CreateDirectory => {
                    let target = safe_join(&plan.destination_root, &item.relative_path)?;
                    if let Err(error) = fs::create_dir_all(&target) {
                        record_error(&job_id, item, error.to_string(), &mut errors);
                    }
                }
                PlanAction::CopyFile => files.push_back(item.clone()),
                PlanAction::Skip => skipped_files += 1,
                PlanAction::Warn => {
                    warn!(%job_id, path = %item.relative_path.display(), reason = %item.reason, "plan warning")
                }
            }
        }

        let files = Arc::new(Mutex::new(files));
        let worker_count = rule.copy.max_concurrency.min(total_files as usize).max(1);
        let (sender, receiver) = mpsc::channel();
        thread::scope(|scope| {
            for _ in 0..worker_count {
                let files = files.clone();
                let sender = sender.clone();
                let source_root = &plan.source_root;
                let destination_root = &plan.destination_root;
                let retry_count = rule.copy.retry_count;
                let preserve_mtime = rule.copy.preserve_mtime;
                scope.spawn(move || {
                    loop {
                        if cancelled.load(Ordering::Relaxed) {
                            break;
                        }
                        let Some(item) = files.lock().expect("copy queue poisoned").pop_front()
                        else {
                            break;
                        };
                        let result = (|| {
                            let source = safe_join(
                                source_root,
                                item.source_relative_path
                                    .as_deref()
                                    .unwrap_or(&item.relative_path),
                            )?;
                            let target = safe_join(destination_root, &item.relative_path)?;
                            copy_with_retries(
                                &source,
                                &target,
                                preserve_mtime,
                                retry_count,
                                job_id,
                                &item,
                            )
                        })()
                        .map_err(|failure: KfcError| failure.to_string());
                        if sender.send((item, result)).is_err() {
                            break;
                        }
                    }
                });
            }
            drop(sender);
            for (item, result) in receiver {
                processed_files += 1;
                match result {
                    Ok(bytes) => {
                        copied_files += 1;
                        copied_bytes += bytes;
                        debug!(%job_id, path = %item.relative_path.display(), bytes, "file copied");
                    }
                    Err(message) => record_error(&job_id, &item, message, &mut errors),
                }
                progress(&ProgressEvent {
                    job_id,
                    relative_path: item.relative_path,
                    completed_files: processed_files,
                    total_files,
                    copied_bytes,
                    total_bytes: plan.stats.bytes_to_copy,
                });
            }
        });

        let status = if cancelled.load(Ordering::Relaxed) {
            warn!(%job_id, "copy job cancelled");
            JobStatus::Cancelled
        } else if errors.is_empty() {
            JobStatus::Succeeded
        } else if copied_files > 0 {
            JobStatus::Partial
        } else {
            JobStatus::Failed
        };
        info!(%job_id, ?status, copied_files, copied_bytes, failures = errors.len(), "copy job finished");
        Ok(JobSummary {
            job_id,
            rule_id: rule.id,
            started_at,
            finished_at: Utc::now(),
            status,
            copied_files,
            skipped_files,
            failed_files: errors.len() as u64,
            copied_bytes,
            errors,
        })
    }
}

fn emit_plan_progress<F>(progress: &mut Option<F>, event: PlanProgressEvent)
where
    F: FnMut(&PlanProgressEvent),
{
    if let Some(progress) = progress {
        progress(&event);
    }
}

fn report_completed_entry<F>(
    progress: &mut Option<F>,
    completed_entries: &mut u64,
    total_entries: Option<u64>,
    relative: &Path,
) where
    F: FnMut(&PlanProgressEvent),
{
    let Some(total_entries) = total_entries else {
        return;
    };
    *completed_entries += 1;
    emit_plan_progress(
        progress,
        PlanProgressEvent {
            phase: PlanProgressPhase::Planning,
            completed_entries: *completed_entries,
            total_entries: Some(total_entries),
            relative_path: Some(relative.to_owned()),
        },
    );
}

/// Renders a mapping's target name from the chosen source file, substituting
/// `{name}`, `{version}`, `{ext}`, and rename-pattern capture groups.
fn render_mapping_target(
    mapping: &VersionedMapping,
    chosen_name: &str,
    pattern: Option<&str>,
    versioned: &VersionedCopyOptions,
) -> Result<PathBuf> {
    let base = base_name_of(chosen_name, pattern).unwrap_or("");
    let version = extract_version(chosen_name, pattern);
    let extension = file_extension(chosen_name);
    let rename_pattern = versioned
        .rename_pattern
        .as_deref()
        .filter(|pattern| !pattern.trim().is_empty());
    let captures = rename_pattern
        .and_then(|rename| capture_groups(rename, chosen_name))
        .unwrap_or_default();
    let rendered = render_target_template(
        &mapping.target_name,
        base,
        version.as_deref(),
        extension,
        &captures,
    );
    Ok(PathBuf::from(normalize_target_name(&rendered)?))
}

/// Auto-derives one mapping per versioned file family found in the source.
/// Files are grouped by base name (version stripped) and the newest version of
/// each family wins. Only files with an extractable version are considered, so
/// unrelated plain files are ignored.
fn build_auto_mappings(
    candidates: &[(PathBuf, String)],
    versioned: &VersionedCopyOptions,
) -> Result<Vec<VersionedMapping>> {
    let pattern = versioned
        .version_pattern
        .as_deref()
        .filter(|pattern| !pattern.trim().is_empty());
    let mut groups: BTreeMap<String, Vec<&(PathBuf, String)>> = BTreeMap::new();
    for candidate in candidates {
        let Some(base) = base_name_of(&candidate.1, pattern) else {
            continue;
        };
        groups.entry(base.to_owned()).or_default().push(candidate);
    }
    let template = versioned
        .target_name_template
        .as_deref()
        .filter(|template| !template.trim().is_empty())
        .unwrap_or("{name}.{ext}");
    let rename_pattern = versioned
        .rename_pattern
        .as_deref()
        .filter(|pattern| !pattern.trim().is_empty());
    let mut mappings = Vec::new();
    for (base, files) in groups {
        let Some(latest) =
            select_versioned_candidate(&files, &VersionSelection::Latest, None, pattern)?
        else {
            continue;
        };
        let extension = file_extension(&latest.1);
        let captures = rename_pattern
            .and_then(|rename| capture_groups(rename, &latest.1))
            .unwrap_or_default();
        let target = render_target_template(
            template,
            &base,
            extract_version(&latest.1, pattern).as_deref(),
            extension,
            &captures,
        );
        normalize_target_name(&target)?;
        mappings.push(VersionedMapping {
            source_pattern: format!(
                "{}_*{}",
                escape_glob(&base),
                if extension.is_empty() {
                    String::new()
                } else {
                    format!(".{extension}")
                }
            ),
            target_name: target,
            selection: VersionSelection::Latest,
            version: None,
            version_pattern: versioned.version_pattern.clone(),
        });
    }
    Ok(mappings)
}

/// Selects the single candidate file a versioned mapping should copy.
/// Returns `None` when nothing matched the mapping pattern.
fn select_versioned_candidate<'a>(
    candidates: &[&'a (PathBuf, String)],
    selection: &VersionSelection,
    exact_version: Option<&str>,
    version_pattern: Option<&str>,
) -> Result<Option<&'a (PathBuf, String)>> {
    if candidates.is_empty() {
        return Ok(None);
    }
    match selection {
        VersionSelection::Latest => {
            let mut versioned = candidates
                .iter()
                .filter_map(|candidate| {
                    extract_version(&candidate.1, version_pattern)
                        .and_then(|text| parse_version(&text))
                        .map(|parsed| (*candidate, parsed))
                })
                .collect::<Vec<_>>();
            versioned.sort_by(|(_, a), (_, b)| b.cmp(a));
            if let Some((candidate, _)) = versioned.first() {
                Ok(Some(candidate))
            } else if candidates.len() == 1 {
                Ok(Some(candidates[0]))
            } else {
                Err(KfcError::InvalidRule(
                    "multiple source files match the pattern but none contains a parseable version"
                        .into(),
                ))
            }
        }
        VersionSelection::Exact => {
            let requested = exact_version.ok_or_else(|| {
                KfcError::InvalidRule("exact version selection requires a version".into())
            })?;
            let requested_parsed = parse_version(requested).ok_or_else(|| {
                KfcError::InvalidRule(format!("unparsable exact version '{requested}'"))
            })?;
            for candidate in candidates {
                if let Some(extracted) = extract_version(&candidate.1, version_pattern)
                    .and_then(|text| parse_version(&text))
                    && extracted == requested_parsed
                {
                    return Ok(Some(candidate));
                }
            }
            Err(KfcError::InvalidRule(format!(
                "no source file has the exact version '{requested}'"
            )))
        }
    }
}

fn should_visit(
    entry: &walkdir::DirEntry,
    source: &Path,
    rule: &CopyRule,
    matcher: &ExclusionMatcher,
) -> bool {
    if entry.path() == source {
        return true;
    }
    entry
        .path()
        .strip_prefix(source)
        .map(|relative| {
            let visible = rule.filters.include_hidden || !is_hidden(relative);
            visible
                && if entry.file_type().is_dir() {
                    matcher.should_descend(relative)
                } else {
                    !matcher.is_excluded(relative)
                }
        })
        .unwrap_or(false)
}

#[derive(Debug, Clone)]
pub struct ValidatedPaths {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub source_is_file: bool,
}

/// The source file a plan entry reads from. `relative` is only set when the
/// source path differs from the destination path (versioned copies).
struct PlannedSource<'a> {
    absolute: &'a Path,
    relative: Option<&'a Path>,
}

fn modified(metadata: &fs::Metadata) -> Option<SystemTime> {
    metadata.modified().ok()
}

fn is_hidden(relative: &Path) -> bool {
    relative.components().any(|component| match component {
        std::path::Component::Normal(name) => name.to_string_lossy().starts_with('.'),
        _ => false,
    })
}

fn safe_join(root: &Path, relative: &Path) -> Result<PathBuf> {
    if relative.is_absolute()
        || relative
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(KfcError::PathEscape(relative.display().to_string()));
    }
    let joined = root.join(relative);
    let mut existing = joined.as_path();
    loop {
        match fs::symlink_metadata(existing) {
            Ok(_) => break,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                existing = existing
                    .parent()
                    .ok_or_else(|| KfcError::PathEscape(joined.display().to_string()))?;
            }
            Err(source) => {
                return Err(KfcError::Io {
                    path: existing.to_owned(),
                    source,
                });
            }
        }
    }
    let resolved = fs::canonicalize(existing).at(existing)?;
    if !resolved.starts_with(root) {
        return Err(KfcError::PathEscape(joined.display().to_string()));
    }
    Ok(joined)
}

fn copy_one(source: &Path, target: &Path, preserve_mtime: bool, job_id: Uuid) -> Result<u64> {
    let parent = target
        .parent()
        .ok_or_else(|| KfcError::PathEscape(target.display().to_string()))?;
    fs::create_dir_all(parent).at(parent)?;
    let name = target
        .file_name()
        .ok_or_else(|| KfcError::PathEscape(target.display().to_string()))?
        .to_string_lossy();
    let temporary = parent.join(format!(".{name}.kfc-part-{job_id}"));
    let backup = parent.join(format!(".{name}.kfc-backup-{job_id}"));
    let before = fs::metadata(source).at(source)?;
    let bytes = fs::copy(source, &temporary).at(&temporary)?;
    let after = fs::metadata(source).at(source)?;
    if before.len() != after.len() || modified(&before) != modified(&after) {
        let _ = fs::remove_file(&temporary);
        return Err(KfcError::InvalidRule(format!(
            "source changed while being copied: {}",
            source.display()
        )));
    }
    if preserve_mtime {
        let mtime = FileTime::from_last_modification_time(&before);
        filetime::set_file_mtime(&temporary, mtime).at(&temporary)?;
    }
    let had_target = target.exists();
    if had_target {
        fs::rename(target, &backup).at(target)?;
    }
    if let Err(source_error) = fs::rename(&temporary, target) {
        if had_target {
            let _ = fs::rename(&backup, target);
        }
        let _ = fs::remove_file(&temporary);
        return Err(KfcError::Io {
            path: target.to_owned(),
            source: source_error,
        });
    }
    if had_target {
        fs::remove_file(&backup).at(&backup)?;
    }
    Ok(bytes)
}

fn copy_with_retries(
    source: &Path,
    target: &Path,
    preserve_mtime: bool,
    retry_count: u32,
    job_id: Uuid,
    item: &PlanItem,
) -> Result<u64> {
    let mut last_error = None;
    for attempt in 0..=retry_count {
        match copy_one(source, target, preserve_mtime, job_id) {
            Ok(bytes) => return Ok(bytes),
            Err(failure) => {
                warn!(%job_id, path = %item.relative_path.display(), attempt, %failure, "copy attempt failed");
                last_error = Some(failure);
            }
        }
    }
    Err(last_error.expect("at least one copy attempt is always made"))
}

fn record_error(job_id: &Uuid, item: &PlanItem, message: String, errors: &mut Vec<FileError>) {
    error!(%job_id, path = %item.relative_path.display(), %message, "file operation failed");
    errors.push(FileError {
        relative_path: item.relative_path.clone(),
        message,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use kfc_core::{
        CopyMode, EndpointConfig, Exclusion, FilterConfig, RULE_SCHEMA_VERSION, TriggerConfig,
        VersionSelection, VersionedCopyOptions, VersionedMapping,
    };

    #[cfg(unix)]
    fn create_directory_link(target: &Path, link: &Path) {
        std::os::unix::fs::symlink(target, link).unwrap();
    }

    #[cfg(windows)]
    fn create_directory_link(target: &Path, link: &Path) {
        let output = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "could not create test junction: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[cfg(unix)]
    fn remove_directory_link(link: &Path) {
        fs::remove_file(link).unwrap();
    }

    #[cfg(windows)]
    fn remove_directory_link(link: &Path) {
        fs::remove_dir(link).unwrap();
    }

    fn temp_root(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("kfc-test-{label}-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn rule(source: PathBuf, destination: PathBuf) -> CopyRule {
        CopyRule {
            schema_version: RULE_SCHEMA_VERSION,
            id: Uuid::new_v4(),
            name: "test".into(),
            enabled: true,
            source: EndpointConfig::Local { path: source },
            destination: EndpointConfig::Local { path: destination },
            filters: FilterConfig::default(),
            copy: Default::default(),
            trigger: TriggerConfig::Manual,
        }
    }

    fn versioned_rule(
        source: PathBuf,
        destination: PathBuf,
        mappings: Vec<VersionedMapping>,
    ) -> CopyRule {
        let mut rule = rule(source, destination);
        rule.copy.mode = CopyMode::Versioned;
        rule.copy.versioned = VersionedCopyOptions {
            mappings,
            target_name_template: None,
            version_pattern: None,
            rename_pattern: None,
        };
        rule
    }

    #[test]
    fn plan_and_execute_incremental_copy() {
        let root = temp_root("copy");
        let source = root.join("source");
        let destination = root.join("destination");
        fs::create_dir_all(source.join("nested")).unwrap();
        fs::create_dir_all(&destination).unwrap();
        fs::write(source.join("nested/file.txt"), b"hello").unwrap();
        let engine = LocalCopyEngine;
        let rule = rule(source, destination.clone());
        let plan = engine.plan(&rule).unwrap();
        assert_eq!(plan.stats.files_to_copy, 1);
        let summary = engine
            .execute(&rule, &plan, &AtomicBool::new(false), |_| {})
            .unwrap();
        assert_eq!(summary.status, JobStatus::Succeeded);
        assert_eq!(
            fs::read(destination.join("nested/file.txt")).unwrap(),
            b"hello"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn excluded_directory_is_pruned() {
        let root = temp_root("exclude");
        let source = root.join("source");
        let destination = root.join("destination");
        fs::create_dir_all(source.join("cache")).unwrap();
        fs::create_dir_all(&destination).unwrap();
        fs::write(source.join("cache/ignored.txt"), b"ignored").unwrap();
        let mut rule = rule(source, destination);
        rule.filters.exclude.push(Exclusion::Path {
            value: "cache".into(),
        });
        let plan = LocalCopyEngine.plan(&rule).unwrap();
        assert_eq!(plan.stats.files_to_copy, 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn included_child_is_scanned_under_excluded_parent() {
        let root = temp_root("include-child");
        let source = root.join("source");
        let destination = root.join("destination");
        fs::create_dir_all(source.join("cache/keep")).unwrap();
        fs::create_dir_all(&destination).unwrap();
        fs::write(source.join("cache/drop.txt"), b"drop").unwrap();
        fs::write(source.join("cache/keep/copy.txt"), b"copy").unwrap();
        let mut rule = rule(source, destination.clone());
        rule.filters.exclude.push(Exclusion::Path {
            value: "cache".into(),
        });
        rule.filters.include.push("cache/keep".into());
        let engine = LocalCopyEngine;
        let plan = engine.plan(&rule).unwrap();

        assert_eq!(plan.stats.files_to_copy, 1);
        let summary = engine
            .execute(&rule, &plan, &AtomicBool::new(false), |_| {})
            .unwrap();
        assert_eq!(summary.status, JobStatus::Succeeded);
        assert!(!destination.join("cache/drop.txt").exists());
        assert_eq!(
            fs::read(destination.join("cache/keep/copy.txt")).unwrap(),
            b"copy"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn planning_reports_discovery_and_determinate_progress() {
        let root = temp_root("plan-progress");
        let source = root.join("source");
        let destination = root.join("destination");
        fs::create_dir_all(source.join("nested")).unwrap();
        fs::create_dir_all(&destination).unwrap();
        fs::write(source.join("one.txt"), b"one").unwrap();
        fs::write(source.join("nested/two.txt"), b"two").unwrap();
        let rule = rule(source, destination);
        let mut events = Vec::new();

        let plan = LocalCopyEngine
            .plan_with_progress(&rule, |event| events.push(event.clone()))
            .unwrap();

        assert_eq!(plan.stats.files_to_copy, 2);
        assert_eq!(events.first().unwrap().phase, PlanProgressPhase::Scanning);
        assert_eq!(events.first().unwrap().completed_entries, 0);
        assert_eq!(events.first().unwrap().total_entries, None);
        let planning = events
            .iter()
            .filter(|event| event.phase == PlanProgressPhase::Planning)
            .collect::<Vec<_>>();
        assert_eq!(planning.first().unwrap().completed_entries, 0);
        assert_eq!(planning.first().unwrap().total_entries, Some(3));
        assert_eq!(planning.last().unwrap().completed_entries, 3);
        assert_eq!(planning.last().unwrap().total_entries, Some(3));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn nested_destination_is_rejected() {
        let root = temp_root("nested-destination");
        let source = root.join("source");
        let destination = source.join("destination");
        fs::create_dir_all(&destination).unwrap();
        let result = LocalCopyEngine.validate_rule(&rule(source, destination));
        assert!(matches!(result, Err(KfcError::UnsafeRelationship(_))));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn single_file_source_is_copied_to_destination_directory() {
        let root = temp_root("single-file");
        let source = root.join("one.txt");
        let destination = root.join("destination");
        fs::write(&source, b"one").unwrap();
        fs::create_dir_all(&destination).unwrap();
        let rule = rule(source, destination.clone());
        let engine = LocalCopyEngine;
        let plan = engine.plan(&rule).unwrap();
        let summary = engine
            .execute(&rule, &plan, &AtomicBool::new(false), |_| {})
            .unwrap();
        assert_eq!(summary.status, JobStatus::Succeeded);
        assert_eq!(fs::read(destination.join("one.txt")).unwrap(), b"one");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn hidden_files_can_be_excluded() {
        let root = temp_root("hidden");
        let source = root.join("source");
        let destination = root.join("destination");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&destination).unwrap();
        fs::write(source.join(".hidden"), b"hidden").unwrap();
        let mut rule = rule(source, destination);
        rule.filters.include_hidden = false;
        assert_eq!(LocalCopyEngine.plan(&rule).unwrap().stats.files_to_copy, 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bounded_workers_copy_multiple_files() {
        let root = temp_root("concurrent");
        let source = root.join("source");
        let destination = root.join("destination");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&destination).unwrap();
        for index in 0..20 {
            fs::write(source.join(format!("{index}.txt")), format!("file-{index}")).unwrap();
        }
        let mut rule = rule(source, destination.clone());
        rule.copy.max_concurrency = 4;
        let engine = LocalCopyEngine;
        let plan = engine.plan(&rule).unwrap();
        let summary = engine
            .execute(&rule, &plan, &AtomicBool::new(false), |_| {})
            .unwrap();
        assert_eq!(summary.copied_files, 20);
        assert_eq!(
            fs::read_to_string(destination.join("19.txt")).unwrap(),
            "file-19"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn pre_cancelled_job_does_not_copy_files() {
        let root = temp_root("cancelled");
        let source = root.join("source");
        let destination = root.join("destination");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&destination).unwrap();
        fs::write(source.join("file.txt"), b"data").unwrap();
        let rule = rule(source, destination.clone());
        let engine = LocalCopyEngine;
        let plan = engine.plan(&rule).unwrap();
        let summary = engine
            .execute(&rule, &plan, &AtomicBool::new(true), |_| {})
            .unwrap();
        assert_eq!(summary.status, JobStatus::Cancelled);
        assert!(!destination.join("file.txt").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn destination_link_escape_is_rejected_during_execution() {
        let root = temp_root("destination-link");
        let source = root.join("source");
        let destination = root.join("destination");
        let outside = root.join("outside");
        fs::create_dir_all(source.join("nested")).unwrap();
        fs::create_dir_all(&destination).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(source.join("nested/file.txt"), b"do not escape").unwrap();
        let rule = rule(source, destination.clone());
        let engine = LocalCopyEngine;
        let plan = engine.plan(&rule).unwrap();
        let link = destination.join("nested");
        create_directory_link(&outside, &link);

        let result = engine.execute(&rule, &plan, &AtomicBool::new(false), |_| {});

        assert!(matches!(result, Err(KfcError::PathEscape(_))));
        assert!(!outside.join("file.txt").exists());
        remove_directory_link(&link);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn versioned_latest_copies_newest_version_to_fixed_name() {
        let root = temp_root("versioned-latest");
        let source = root.join("source");
        let destination = root.join("destination");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&destination).unwrap();
        fs::write(source.join("EDM.DSP.Core_1.0.0.dll"), b"v1").unwrap();
        fs::write(source.join("EDM.DSP.Core_1.10.0.dll"), b"v1.10").unwrap();
        fs::write(source.join("EDM.DSP.Core_2.0.0.dll"), b"v2").unwrap();
        let rule = versioned_rule(
            source.clone(),
            destination.clone(),
            vec![VersionedMapping {
                source_pattern: "EDM.DSP.Core*.dll".into(),
                target_name: "EDM.DSP.Core.dll".into(),
                selection: VersionSelection::Latest,
                version: None,
                version_pattern: None,
            }],
        );

        let engine = LocalCopyEngine;
        let plan = engine.plan(&rule).unwrap();
        assert_eq!(plan.stats.files_to_copy, 1);
        assert_eq!(plan.items[0].action, PlanAction::CopyFile);
        assert_eq!(plan.items[0].relative_path, Path::new("EDM.DSP.Core.dll"));
        assert_eq!(
            plan.items[0].source_relative_path.as_deref(),
            Some(Path::new("EDM.DSP.Core_2.0.0.dll"))
        );

        engine
            .execute(&rule, &plan, &AtomicBool::new(false), |_| {})
            .unwrap();
        assert_eq!(
            fs::read(destination.join("EDM.DSP.Core.dll")).unwrap(),
            b"v2"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn versioned_exact_copies_the_requested_version() {
        let root = temp_root("versioned-exact");
        let source = root.join("source");
        let destination = root.join("destination");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&destination).unwrap();
        fs::write(source.join("EDM.DSP.Proxy_1.0.0.dll"), b"old").unwrap();
        fs::write(source.join("EDM.DSP.Proxy_2.0.0.dll"), b"new").unwrap();
        let rule = versioned_rule(
            source.clone(),
            destination.clone(),
            vec![VersionedMapping {
                source_pattern: "EDM.DSP.Proxy*.dll".into(),
                target_name: "EDM.DSP.Proxy.dll".into(),
                selection: VersionSelection::Exact,
                version: Some("1.0.0".into()),
                version_pattern: None,
            }],
        );

        let engine = LocalCopyEngine;
        let plan = engine.plan(&rule).unwrap();
        assert_eq!(plan.stats.files_to_copy, 1);
        assert_eq!(
            plan.items[0].source_relative_path.as_deref(),
            Some(Path::new("EDM.DSP.Proxy_1.0.0.dll"))
        );

        engine
            .execute(&rule, &plan, &AtomicBool::new(false), |_| {})
            .unwrap();
        assert_eq!(
            fs::read(destination.join("EDM.DSP.Proxy.dll")).unwrap(),
            b"old"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn versioned_single_unversioned_file_maps_to_fixed_name() {
        let root = temp_root("versioned-single");
        let source = root.join("source");
        let destination = root.join("destination");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&destination).unwrap();
        fs::write(source.join("EDM.DSP.Gateway.dll"), b"gateway").unwrap();
        let rule = versioned_rule(
            source.clone(),
            destination.clone(),
            vec![VersionedMapping {
                source_pattern: "EDM.DSP.Gateway*.dll".into(),
                target_name: "bin/EDM.DSP.Gateway.dll".into(),
                selection: VersionSelection::Latest,
                version: None,
                version_pattern: None,
            }],
        );

        let engine = LocalCopyEngine;
        let plan = engine.plan(&rule).unwrap();
        assert_eq!(plan.stats.files_to_copy, 1);
        assert_eq!(
            plan.items[0].relative_path,
            Path::new("bin/EDM.DSP.Gateway.dll")
        );

        engine
            .execute(&rule, &plan, &AtomicBool::new(false), |_| {})
            .unwrap();
        assert_eq!(
            fs::read(destination.join("bin/EDM.DSP.Gateway.dll")).unwrap(),
            b"gateway"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn versioned_missing_match_emits_a_warning() {
        let root = temp_root("versioned-missing");
        let source = root.join("source");
        let destination = root.join("destination");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&destination).unwrap();
        fs::write(source.join("EDM.DSP.Core_1.0.0.dll"), b"v1").unwrap();
        let rule = versioned_rule(
            source,
            destination,
            vec![VersionedMapping {
                source_pattern: "EDM.DSP.Web*.dll".into(),
                target_name: "EDM.DSP.Web.dll".into(),
                selection: VersionSelection::Latest,
                version: None,
                version_pattern: None,
            }],
        );

        let plan = LocalCopyEngine.plan(&rule).unwrap();
        assert_eq!(plan.stats.files_to_copy, 0);
        assert_eq!(plan.stats.warnings, 1);
        assert_eq!(plan.items[0].action, PlanAction::Warn);
        assert_eq!(plan.items[0].relative_path, Path::new("EDM.DSP.Web.dll"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn versioned_multiple_unversioned_matches_are_rejected() {
        let root = temp_root("versioned-ambiguous");
        let source = root.join("source");
        let destination = root.join("destination");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&destination).unwrap();
        fs::write(source.join("EDM.DSP.Core.dll"), b"core").unwrap();
        fs::write(source.join("EDM.DSP.Core.Data.dll"), b"data").unwrap();
        let rule = versioned_rule(
            source,
            destination,
            vec![VersionedMapping {
                source_pattern: "EDM.DSP.Core*.dll".into(),
                target_name: "EDM.DSP.Core.dll".into(),
                selection: VersionSelection::Latest,
                version: None,
                version_pattern: None,
            }],
        );

        let result = LocalCopyEngine.plan(&rule);
        assert!(matches!(result, Err(KfcError::InvalidRule(_))));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn versioned_exact_version_not_found_is_rejected() {
        let root = temp_root("versioned-exact-missing");
        let source = root.join("source");
        let destination = root.join("destination");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&destination).unwrap();
        fs::write(source.join("EDM.DSP.Proxy_1.0.0.dll"), b"v1").unwrap();
        let rule = versioned_rule(
            source,
            destination,
            vec![VersionedMapping {
                source_pattern: "EDM.DSP.Proxy*.dll".into(),
                target_name: "EDM.DSP.Proxy.dll".into(),
                selection: VersionSelection::Exact,
                version: Some("9.9.9".into()),
                version_pattern: None,
            }],
        );

        let result = LocalCopyEngine.plan(&rule);
        assert!(matches!(result, Err(KfcError::InvalidRule(_))));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn versioned_auto_mode_derives_families_and_picks_newest() {
        let root = temp_root("versioned-auto");
        let source = root.join("source");
        let destination = root.join("destination");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&destination).unwrap();
        fs::write(source.join("80R_12.1.0.1.bin"), b"r1").unwrap();
        fs::write(source.join("80R_12.1.8.72.bin"), b"r72").unwrap();
        fs::write(source.join("80S_12.1.3.4.bin"), b"s4").unwrap();
        fs::write(source.join("80S_12.1.8.50.bin"), b"s50").unwrap();
        fs::write(source.join("readme.txt"), b"ignored").unwrap();

        let mut rule = rule(source.clone(), destination.clone());
        rule.copy.mode = CopyMode::Versioned;
        rule.copy.versioned = VersionedCopyOptions::default();

        let engine = LocalCopyEngine;
        let plan = engine.plan(&rule).unwrap();
        assert_eq!(plan.stats.files_to_copy, 2);
        assert_eq!(plan.stats.warnings, 0);
        let copy_items: Vec<_> = plan
            .items
            .iter()
            .filter(|item| item.action == PlanAction::CopyFile)
            .collect();
        assert_eq!(copy_items.len(), 2);
        let r_item = copy_items
            .iter()
            .find(|item| item.relative_path == Path::new("80R.bin"))
            .unwrap();
        assert_eq!(
            r_item.source_relative_path.as_deref(),
            Some(Path::new("80R_12.1.8.72.bin"))
        );
        let s_item = copy_items
            .iter()
            .find(|item| item.relative_path == Path::new("80S.bin"))
            .unwrap();
        assert_eq!(
            s_item.source_relative_path.as_deref(),
            Some(Path::new("80S_12.1.8.50.bin"))
        );

        engine
            .execute(&rule, &plan, &AtomicBool::new(false), |_| {})
            .unwrap();
        assert_eq!(fs::read(destination.join("80R.bin")).unwrap(), b"r72");
        assert_eq!(fs::read(destination.join("80S.bin")).unwrap(), b"s50");
        assert!(!destination.join("readme.txt").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn versioned_auto_mode_rename_template_uses_capture_groups() {
        let root = temp_root("versioned-rename");
        let source = root.join("source");
        let destination = root.join("destination");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&destination).unwrap();
        fs::write(source.join("80R_12.1.8.72.bin"), b"r").unwrap();
        fs::write(source.join("80X_12.1.8.40.bin"), b"x").unwrap();
        fs::write(source.join("80SG_12.1.5.12.bin"), b"sg").unwrap();

        let mut rule = rule(source, destination);
        rule.copy.mode = CopyMode::Versioned;
        rule.copy.versioned = VersionedCopyOptions {
            mappings: Vec::new(),
            target_name_template: Some("EDM.BIN0003{1}.dll".into()),
            version_pattern: None,
            rename_pattern: Some(r"^80([A-Za-z]+)_".into()),
        };

        let plan = LocalCopyEngine.plan(&rule).unwrap();
        let targets: Vec<_> = plan
            .items
            .iter()
            .filter(|item| item.action == PlanAction::CopyFile)
            .map(|item| item.relative_path.clone())
            .collect();
        assert_eq!(
            targets,
            vec![
                PathBuf::from("EDM.BIN0003R.dll"),
                PathBuf::from("EDM.BIN0003SG.dll"),
                PathBuf::from("EDM.BIN0003X.dll"),
            ]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn detect_versioned_mappings_returns_suggested_mapping_list() {
        let root = temp_root("versioned-detect");
        let source = root.join("source");
        let destination = root.join("destination");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&destination).unwrap();
        fs::write(source.join("EDM.DSP.Core_1.0.0.dll"), b"v1").unwrap();
        fs::write(source.join("EDM.DSP.Core_2.0.0.dll"), b"v2").unwrap();
        fs::write(source.join("EDM.DSP.Proxy_1.1.0.dll"), b"p1").unwrap();
        fs::write(source.join("notes.txt"), b"ignored").unwrap();

        let mut rule = rule(source, destination);
        rule.copy.mode = CopyMode::Versioned;
        rule.copy.versioned = VersionedCopyOptions::default();

        let detected = LocalCopyEngine.detect_versioned_mappings(&rule).unwrap();
        let names: Vec<_> = detected
            .iter()
            .map(|mapping| (mapping.target_name.clone(), mapping.selection.clone()))
            .collect();
        assert_eq!(
            names,
            vec![
                ("EDM.DSP.Core.dll".to_owned(), VersionSelection::Latest),
                ("EDM.DSP.Proxy.dll".to_owned(), VersionSelection::Latest),
            ]
        );
        assert!(
            detected
                .iter()
                .all(|mapping| mapping.source_pattern == "EDM.DSP.Core_*.dll"
                    || mapping.source_pattern == "EDM.DSP.Proxy_*.dll")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explicit_mapping_target_name_supports_placeholders() {
        let root = temp_root("versioned-placeholder");
        let source = root.join("source");
        let destination = root.join("destination");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&destination).unwrap();
        fs::write(source.join("80R_12.1.8.72.bin"), b"r").unwrap();

        let rule = versioned_rule(
            source,
            destination,
            vec![VersionedMapping {
                source_pattern: "80R_*.bin".into(),
                target_name: "EDM.BIN0003{name}.dll".into(),
                selection: VersionSelection::Latest,
                version: None,
                version_pattern: None,
            }],
        );
        let plan = LocalCopyEngine.plan(&rule).unwrap();
        assert_eq!(plan.stats.files_to_copy, 1);
        assert_eq!(plan.items[0].relative_path, Path::new("EDM.BIN000380R.dll"));
        fs::remove_dir_all(root).unwrap();
    }
}
