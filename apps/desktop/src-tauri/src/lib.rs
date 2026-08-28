use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use chrono::Utc;
use kfc_core::{CopyPlan, CopyRule, JobSummary, PlanAction, ProgressEvent, TriggerConfig};
use kfc_local_endpoint::LocalCopyEngine;
use kfc_persistence::{FileRepositories, JobEvent, PersistenceError};
use notify::{RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, State};
use walkdir::WalkDir;

const RECENT_PATH_LIMIT: usize = 10;

struct AppState {
    repositories: FileRepositories,
    cancelled: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceEntry {
    relative_path: String,
    directory: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RecentPaths {
    #[serde(default)]
    sources: Vec<PathBuf>,
    #[serde(default)]
    destinations: Vec<PathBuf>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum RecentPathKind {
    Source,
    Destination,
}

#[tauri::command]
fn list_rules(state: State<'_, AppState>) -> Result<Vec<CopyRule>, String> {
    state.repositories.list_rules().map_err(error)
}

#[tauri::command]
fn get_recent_paths(state: State<'_, AppState>) -> Result<RecentPaths, String> {
    load_recent_paths(&state.repositories)
}

#[tauri::command]
fn remember_recent_path(
    state: State<'_, AppState>,
    kind: RecentPathKind,
    path: String,
) -> Result<RecentPaths, String> {
    let path = PathBuf::from(path.trim());
    if path.as_os_str().is_empty() {
        return Err("recent path must not be empty".into());
    }
    let mut recent = load_recent_paths(&state.repositories)?;
    let paths = match kind {
        RecentPathKind::Source => &mut recent.sources,
        RecentPathKind::Destination => &mut recent.destinations,
    };
    push_recent_path(paths, path);
    state.repositories.save_settings(&recent).map_err(error)?;
    Ok(recent)
}

#[tauri::command]
fn save_rule(state: State<'_, AppState>, rule: CopyRule) -> Result<(), String> {
    validate_rule_full(&rule)?;
    state.repositories.save_rule(&rule).map_err(error)
}

#[tauri::command]
fn delete_rule(state: State<'_, AppState>, id: uuid::Uuid) -> Result<(), String> {
    state.repositories.delete_rule(id).map_err(error)
}

#[tauri::command]
fn validate_rule(rule: CopyRule) -> Result<(), String> {
    validate_rule_full(&rule)
}

#[tauri::command]
async fn preview_rule(rule: CopyRule) -> Result<CopyPlan, String> {
    tauri::async_runtime::spawn_blocking(move || LocalCopyEngine.plan(&rule))
        .await
        .map_err(error)?
        .map_err(error)
}

#[tauri::command]
async fn execute_rule(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    rule: CopyRule,
) -> Result<JobSummary, String> {
    let repositories = state.repositories.clone();
    let cancelled = state.cancelled.clone();
    let running = state.running.clone();
    tauri::async_runtime::spawn_blocking(move || {
        run_copy_job(app, repositories, rule, cancelled, running)
    })
    .await
    .map_err(error)?
}

fn run_copy_job(
    app: tauri::AppHandle,
    repositories: FileRepositories,
    rule: CopyRule,
    cancelled: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
) -> Result<JobSummary, String> {
    if running.swap(true, Ordering::SeqCst) {
        return Err("another copy job is already running".into());
    }
    cancelled.store(false, Ordering::SeqCst);
    let event_repositories = repositories.clone();
    let app_for_events = app.clone();
    let result = (|| {
        let engine = LocalCopyEngine;
        let plan = engine.plan(&rule).map_err(error)?;
        let summary = engine
            .execute(&rule, &plan, &cancelled, |event: &ProgressEvent| {
                let _ = app_for_events.emit("job-progress", event);
                let _ = event_repositories.append_job_event(
                    event.job_id,
                    &JobEvent {
                        timestamp: Utc::now(),
                        level: "info".into(),
                        phase: "running".into(),
                        relative_path: Some(event.relative_path.clone()),
                        operation: "copy-file".into(),
                        result: "processed".into(),
                        message: None,
                    },
                );
            })
            .map_err(error)?;
        Ok::<_, String>((summary, plan))
    })();
    running.store(false, Ordering::SeqCst);
    let (summary, plan) = result?;
    for item in &plan.items {
        let (operation, result) = match item.action {
            PlanAction::Skip => ("skip", "skipped"),
            PlanAction::Warn => ("warning", "warning"),
            PlanAction::CreateDirectory => ("create-directory", "planned"),
            PlanAction::CopyFile => continue,
        };
        repositories
            .append_job_event(
                summary.job_id,
                &JobEvent {
                    timestamp: summary.finished_at,
                    level: if matches!(item.action, PlanAction::Warn) {
                        "warn".into()
                    } else {
                        "info".into()
                    },
                    phase: "finished".into(),
                    relative_path: Some(item.relative_path.clone()),
                    operation: operation.into(),
                    result: result.into(),
                    message: Some(item.reason.clone()),
                },
            )
            .map_err(error)?;
    }
    repositories
        .append_job_event(
            summary.job_id,
            &JobEvent {
                timestamp: summary.finished_at,
                level: "info".into(),
                phase: "finished".into(),
                relative_path: None,
                operation: "job".into(),
                result: format!("{:?}", summary.status).to_lowercase(),
                message: Some(format!(
                    "{} copied, {} skipped, {} failed",
                    summary.copied_files, summary.skipped_files, summary.failed_files
                )),
            },
        )
        .map_err(error)?;
    for file_error in &summary.errors {
        repositories
            .append_job_event(
                summary.job_id,
                &JobEvent {
                    timestamp: summary.finished_at,
                    level: "error".into(),
                    phase: "running".into(),
                    relative_path: Some(file_error.relative_path.clone()),
                    operation: "copy-file".into(),
                    result: "failed".into(),
                    message: Some(file_error.message.clone()),
                },
            )
            .map_err(error)?;
    }
    repositories.save_job_summary(&summary).map_err(error)?;
    let _ = app.emit("job-finished", &summary);
    Ok(summary)
}

#[tauri::command]
fn cancel_job(state: State<'_, AppState>) {
    state.cancelled.store(true, Ordering::SeqCst);
}

#[tauri::command]
fn list_jobs(state: State<'_, AppState>) -> Result<Vec<JobSummary>, String> {
    state.repositories.list_job_summaries().map_err(error)
}

#[tauri::command]
fn get_job_events(
    state: State<'_, AppState>,
    id: uuid::Uuid,
) -> Result<Vec<serde_json::Value>, String> {
    state.repositories.read_job_events(id).map_err(error)
}

#[tauri::command]
fn list_source_entries(rule: CopyRule) -> Result<Vec<SourceEntry>, String> {
    let paths = LocalCopyEngine.validate_rule(&rule).map_err(error)?;
    if paths.source_is_file {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    for entry in WalkDir::new(&paths.source)
        .follow_links(false)
        .max_depth(12)
        .into_iter()
        .skip(1)
        .take(5000)
    {
        let entry = entry.map_err(error)?;
        let relative = entry.path().strip_prefix(&paths.source).map_err(error)?;
        entries.push(SourceEntry {
            relative_path: relative.to_string_lossy().replace('\\', "/"),
            directory: entry.file_type().is_dir(),
        });
    }
    Ok(entries)
}

fn load_recent_paths(repositories: &FileRepositories) -> Result<RecentPaths, String> {
    match repositories.load_settings() {
        Ok(recent) => Ok(recent),
        Err(PersistenceError::Io { source, .. })
            if source.kind() == std::io::ErrorKind::NotFound =>
        {
            Ok(RecentPaths::default())
        }
        Err(failure) => Err(error(failure)),
    }
}

fn push_recent_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    paths.retain(|existing| !same_recent_path(existing, &path));
    paths.insert(0, path);
    paths.truncate(RECENT_PATH_LIMIT);
}

#[cfg(windows)]
fn same_recent_path(left: &Path, right: &Path) -> bool {
    left.to_string_lossy()
        .eq_ignore_ascii_case(&right.to_string_lossy())
}

#[cfg(not(windows))]
fn same_recent_path(left: &Path, right: &Path) -> bool {
    left == right
}

fn error(value: impl std::fmt::Display) -> String {
    value.to_string()
}

fn validate_rule_full(rule: &CopyRule) -> Result<(), String> {
    LocalCopyEngine.validate_rule(rule).map_err(error)?;
    if let TriggerConfig::Cron { expression, .. } = &rule.trigger {
        kfc_scheduler::next_cron_time(expression, Utc::now()).map_err(error)?;
    }
    Ok(())
}

fn spawn_scheduler(
    app: tauri::AppHandle,
    repositories: FileRepositories,
    cancelled: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
) {
    thread::spawn(move || {
        let (sender, receiver) = mpsc::channel();
        let mut watcher =
            match notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                let _ = sender.send(event);
            }) {
                Ok(watcher) => watcher,
                Err(failure) => {
                    let _ = app.emit("scheduler-error", failure.to_string());
                    return;
                }
            };

        let mut rules = Vec::<CopyRule>::new();
        let mut watched = HashSet::<PathBuf>::new();
        let mut versions = HashMap::<uuid::Uuid, String>::new();
        let mut watch_due = HashMap::<uuid::Uuid, Instant>::new();
        let mut interval_due = HashMap::<uuid::Uuid, Instant>::new();
        let mut cron_due = HashMap::<uuid::Uuid, chrono::DateTime<Utc>>::new();
        let mut last_reload = Instant::now() - Duration::from_secs(10);

        loop {
            if last_reload.elapsed() >= Duration::from_secs(2) {
                let loaded = match repositories.list_rules() {
                    Ok(loaded) => loaded
                        .into_iter()
                        .filter(|rule| rule.enabled)
                        .collect::<Vec<_>>(),
                    Err(failure) => {
                        let _ = app.emit("scheduler-error", failure.to_string());
                        Vec::new()
                    }
                };
                let desired = loaded
                    .iter()
                    .filter(|rule| matches!(rule.trigger, TriggerConfig::Watch { .. }))
                    .map(|rule| rule.source.path().clone())
                    .collect::<HashSet<_>>();
                for path in watched.difference(&desired) {
                    let _ = watcher.unwatch(path);
                }
                let mut active_watches = watched
                    .intersection(&desired)
                    .cloned()
                    .collect::<HashSet<_>>();
                for path in desired.difference(&watched) {
                    match watcher.watch(path, RecursiveMode::Recursive) {
                        Ok(()) => {
                            active_watches.insert(path.clone());
                        }
                        Err(failure) => {
                            let _ = app.emit(
                                "scheduler-error",
                                format!("could not watch {}: {failure}", path.display()),
                            );
                        }
                    }
                }
                watched = active_watches;

                let active_ids = loaded.iter().map(|rule| rule.id).collect::<HashSet<_>>();
                watch_due.retain(|id, _| active_ids.contains(id));
                interval_due.retain(|id, _| active_ids.contains(id));
                cron_due.retain(|id, _| active_ids.contains(id));
                versions.retain(|id, _| active_ids.contains(id));
                for rule in &loaded {
                    let version = serde_json::to_string(&rule.trigger).unwrap_or_default();
                    let changed = versions.get(&rule.id) != Some(&version);
                    if changed {
                        interval_due.remove(&rule.id);
                        cron_due.remove(&rule.id);
                        watch_due.remove(&rule.id);
                    }
                    match &rule.trigger {
                        TriggerConfig::Interval { seconds } => {
                            interval_due
                                .entry(rule.id)
                                .or_insert_with(|| Instant::now() + Duration::from_secs(*seconds));
                        }
                        TriggerConfig::Cron { expression, .. } => {
                            if let std::collections::hash_map::Entry::Vacant(entry) =
                                cron_due.entry(rule.id)
                            {
                                match kfc_scheduler::next_cron_time(expression, Utc::now()) {
                                    Ok(Some(next)) => {
                                        entry.insert(next);
                                    }
                                    Ok(None) => {}
                                    Err(failure) => {
                                        let _ = app.emit("scheduler-error", failure.to_string());
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                    versions.insert(rule.id, version);
                }
                rules = loaded;
                last_reload = Instant::now();
            }

            match receiver.recv_timeout(Duration::from_millis(250)) {
                Ok(Ok(event)) => {
                    let now = Instant::now();
                    for rule in &rules {
                        if let TriggerConfig::Watch { debounce_ms } = rule.trigger
                            && event
                                .paths
                                .iter()
                                .any(|path| path.starts_with(rule.source.path()))
                        {
                            watch_due.insert(rule.id, now + Duration::from_millis(debounce_ms));
                        }
                    }
                }
                Ok(Err(failure)) => {
                    let _ = app.emit("scheduler-error", failure.to_string());
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }

            let now = Instant::now();
            let utc_now = Utc::now();
            for rule in &rules {
                let due = watch_due.get(&rule.id).is_some_and(|due| *due <= now)
                    || interval_due.get(&rule.id).is_some_and(|due| *due <= now)
                    || cron_due.get(&rule.id).is_some_and(|due| *due <= utc_now);
                if !due {
                    continue;
                }
                let result = run_copy_job(
                    app.clone(),
                    repositories.clone(),
                    rule.clone(),
                    cancelled.clone(),
                    running.clone(),
                );
                if result
                    .as_ref()
                    .is_err_and(|failure| failure.contains("already running"))
                {
                    continue;
                }
                if let Err(failure) = result {
                    let _ = app.emit("scheduler-error", failure);
                }
                watch_due.remove(&rule.id);
                match &rule.trigger {
                    TriggerConfig::Interval { seconds } => {
                        interval_due
                            .insert(rule.id, Instant::now() + Duration::from_secs(*seconds));
                    }
                    TriggerConfig::Cron { expression, .. } => {
                        cron_due.remove(&rule.id);
                        if let Ok(Some(next)) =
                            kfc_scheduler::next_cron_time(expression, Utc::now())
                        {
                            cron_due.insert(rule.id, next);
                        }
                    }
                    _ => {}
                }
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let repositories = FileRepositories::new(data_dir);
            repositories
                .initialize()
                .map_err(Box::<dyn std::error::Error>::from)?;
            let _ = repositories.remove_jobs_older_than(Utc::now() - chrono::Duration::days(30));
            let cancelled = Arc::new(AtomicBool::new(false));
            let running = Arc::new(AtomicBool::new(false));
            app.manage(AppState {
                repositories: repositories.clone(),
                cancelled: cancelled.clone(),
                running: running.clone(),
            });
            spawn_scheduler(app.handle().clone(), repositories, cancelled, running);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_rules,
            get_recent_paths,
            remember_recent_path,
            save_rule,
            delete_rule,
            validate_rule,
            preview_rule,
            execute_rule,
            cancel_job,
            list_jobs,
            get_job_events,
            list_source_entries
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Kaven File Copy");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_paths_are_newest_first_deduplicated_and_bounded() {
        let mut paths = Vec::new();
        push_recent_path(&mut paths, PathBuf::from("first"));
        push_recent_path(&mut paths, PathBuf::from("second"));
        push_recent_path(&mut paths, PathBuf::from("first"));
        assert_eq!(paths, vec![PathBuf::from("first"), PathBuf::from("second")]);

        for index in 0..RECENT_PATH_LIMIT + 2 {
            push_recent_path(&mut paths, PathBuf::from(format!("path-{index}")));
        }
        assert_eq!(paths.len(), RECENT_PATH_LIMIT);
        assert_eq!(paths.first(), Some(&PathBuf::from("path-11")));
        assert_eq!(paths.last(), Some(&PathBuf::from("path-2")));
    }

    #[cfg(windows)]
    #[test]
    fn recent_paths_are_case_insensitive_on_windows() {
        let mut paths = vec![PathBuf::from(r"C:\Work\Source")];
        push_recent_path(&mut paths, PathBuf::from(r"c:\work\source"));
        assert_eq!(paths, vec![PathBuf::from(r"c:\work\source")]);
    }

    #[test]
    fn recent_paths_round_trip_through_settings_repository() {
        let root = std::env::temp_dir().join(format!("kfc-recent-test-{}", uuid::Uuid::new_v4()));
        let repositories = FileRepositories::new(&root);
        let recent = RecentPaths {
            sources: vec![PathBuf::from("source")],
            destinations: vec![PathBuf::from("destination")],
        };
        repositories.save_settings(&recent).unwrap();

        let loaded = load_recent_paths(&repositories).unwrap();

        assert_eq!(loaded.sources, recent.sources);
        assert_eq!(loaded.destinations, recent.destinations);
        std::fs::remove_dir_all(root).unwrap();
    }
}
