mod endpoint;

pub use endpoint::LocalEndpoint;

use std::{
    collections::VecDeque,
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
use kfc_core::{
    CompareStrategy, CopyPlan, CopyRule, ExclusionMatcher, FileError, IoResultExt, JobStatus,
    JobSummary, KfcError, OverwritePolicy, PlanAction, PlanItem, PlanProgressEvent,
    PlanProgressPhase, PlanStats, ProgressEvent, Result,
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
                    &paths.source,
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
                            size: 0,
                            reason: "destination directory does not exist".into(),
                        });
                    }
                } else if file_type.is_file() {
                    self.plan_file(
                        rule,
                        entry.path(),
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

    fn plan_file(
        &self,
        rule: &CopyRule,
        source: &Path,
        relative: &Path,
        destination_root: &Path,
        items: &mut Vec<PlanItem>,
        stats: &mut PlanStats,
    ) -> Result<()> {
        let source_metadata = fs::metadata(source).at(source)?;
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
                            let source = safe_join(source_root, &item.relative_path)?;
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
    use kfc_core::{EndpointConfig, Exclusion, FilterConfig, RULE_SCHEMA_VERSION, TriggerConfig};

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
}
