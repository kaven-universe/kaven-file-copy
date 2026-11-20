use std::{
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
};

use chrono::{DateTime, Utc};
use kfc_core::{CopyRule, JobSummary};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use tracing::warn;
use uuid::Uuid;

const MAX_CONFIG_BYTES: u64 = 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum PersistenceError {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid JSON at {path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("file at {path} is {size} bytes, exceeding the {limit}-byte limit")]
    TooLarge {
        path: PathBuf,
        size: u64,
        limit: u64,
    },
    #[error("failed to restore {path} from backup {backup}: {source}")]
    Recovery {
        path: PathBuf,
        backup: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub type Result<T> = std::result::Result<T, PersistenceError>;

#[derive(Debug, Clone)]
pub struct FileRepositories {
    root: PathBuf,
}

impl FileRepositories {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn initialize(&self) -> Result<()> {
        create_dir_all(&self.rules_dir())?;
        create_dir_all(&self.jobs_dir())
    }

    pub fn save_rule(&self, rule: &CopyRule) -> Result<()> {
        self.initialize()?;
        write_json_atomic(&self.rule_path(rule.id), rule)
    }

    pub fn load_rule(&self, id: Uuid) -> Result<CopyRule> {
        read_json(&self.rule_path(id))
    }

    pub fn delete_rule(&self, id: Uuid) -> Result<()> {
        let path = self.rule_path(id);
        fs::remove_file(&path).map_err(|source| PersistenceError::Io { path, source })
    }

    pub fn list_rules(&self) -> Result<Vec<CopyRule>> {
        self.initialize()?;
        let mut rules: Vec<CopyRule> = Vec::new();
        for entry in read_dir(&self.rules_dir())? {
            let entry = entry.map_err(|source| PersistenceError::Io {
                path: self.rules_dir(),
                source,
            })?;
            if entry.path().extension().and_then(|value| value.to_str()) == Some("json") {
                match read_json(&entry.path()) {
                    Ok(rule) => rules.push(rule),
                    Err(failure) => {
                        warn!(path = %entry.path().display(), %failure, "skipping unreadable rule");
                    }
                }
            }
        }
        rules.sort_by_key(|rule| rule.name.to_lowercase());
        Ok(rules)
    }

    pub fn save_settings<T: Serialize>(&self, settings: &T) -> Result<()> {
        create_dir_all(&self.root)?;
        write_json_atomic(&self.root.join("config.json"), settings)
    }

    pub fn load_settings<T: DeserializeOwned>(&self) -> Result<T> {
        read_json_limited(&self.root.join("config.json"), MAX_CONFIG_BYTES)
    }

    pub fn save_job_summary(&self, summary: &JobSummary) -> Result<()> {
        let directory = self.job_dir(summary.job_id);
        create_dir_all(&directory)?;
        write_json_atomic(&directory.join("summary.json"), summary)
    }

    pub fn append_job_event<T: Serialize>(&self, job_id: Uuid, event: &T) -> Result<()> {
        let directory = self.job_dir(job_id);
        create_dir_all(&directory)?;
        let path = directory.join("events.jsonl");
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|source| PersistenceError::Io {
                path: path.clone(),
                source,
            })?;
        serde_json::to_writer(&mut file, event).map_err(|source| PersistenceError::Json {
            path: path.clone(),
            source,
        })?;
        file.write_all(b"\n")
            .map_err(|source| PersistenceError::Io {
                path: path.clone(),
                source,
            })?;
        file.flush()
            .map_err(|source| PersistenceError::Io { path, source })
    }

    pub fn list_job_summaries(&self) -> Result<Vec<JobSummary>> {
        self.initialize()?;
        let mut jobs: Vec<JobSummary> = Vec::new();
        for entry in read_dir(&self.jobs_dir())? {
            let entry = entry.map_err(|source| PersistenceError::Io {
                path: self.jobs_dir(),
                source,
            })?;
            let summary = entry.path().join("summary.json");
            if summary.is_file() {
                match read_json(&summary) {
                    Ok(job) => jobs.push(job),
                    Err(failure) => {
                        warn!(path = %summary.display(), %failure, "skipping unreadable job summary");
                    }
                }
            }
        }
        jobs.sort_by_key(|job| std::cmp::Reverse(job.started_at));
        Ok(jobs)
    }

    pub fn read_job_events(&self, job_id: Uuid) -> Result<Vec<serde_json::Value>> {
        let path = self.job_dir(job_id).join("events.jsonl");
        let file = fs::File::open(&path).map_err(|source| PersistenceError::Io {
            path: path.clone(),
            source,
        })?;
        BufReader::new(file)
            .lines()
            .map(|line| {
                let line = line.map_err(|source| PersistenceError::Io {
                    path: path.clone(),
                    source,
                })?;
                serde_json::from_str(&line).map_err(|source| PersistenceError::Json {
                    path: path.clone(),
                    source,
                })
            })
            .collect()
    }

    /// Removes a single job and its recorded events from history.
    pub fn delete_job(&self, id: Uuid) -> Result<()> {
        let path = self.job_dir(id);
        match fs::remove_dir_all(&path) {
            Ok(()) => Ok(()),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(PersistenceError::Io { path, source }),
        }
    }

    /// Removes every recorded job from history.
    pub fn clear_jobs(&self) -> Result<()> {
        let path = self.jobs_dir();
        match fs::remove_dir_all(&path) {
            Ok(()) => {}
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => return Err(PersistenceError::Io { path, source }),
        }
        create_dir_all(&path)
    }

    fn rules_dir(&self) -> PathBuf {
        self.root.join("data/rules")
    }
    fn jobs_dir(&self) -> PathBuf {
        self.root.join("data/jobs")
    }
    fn rule_path(&self, id: Uuid) -> PathBuf {
        self.rules_dir().join(format!("{id}.json"))
    }
    fn job_dir(&self, id: Uuid) -> PathBuf {
        self.jobs_dir().join(id.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobEvent {
    pub timestamp: DateTime<Utc>,
    pub level: String,
    pub phase: String,
    pub relative_path: Option<PathBuf>,
    pub operation: String,
    pub result: String,
    pub message: Option<String>,
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    create_dir_all(parent)?;
    let transaction_id = Uuid::new_v4();
    let temporary = path.with_extension(format!("json.{transaction_id}.tmp"));
    if let Err(failure) = write_json_temporary(&temporary, value) {
        let _ = fs::remove_file(&temporary);
        return Err(failure);
    }
    if path.exists() {
        let backup = path.with_extension(format!("json.{transaction_id}.backup"));
        if let Err(source) = fs::rename(path, &backup) {
            let _ = fs::remove_file(&temporary);
            return Err(PersistenceError::Io {
                path: path.to_owned(),
                source,
            });
        }
        if let Err(failure) = sync_parent_directory(parent) {
            let _ = fs::remove_file(&temporary);
            if let Err(source) = fs::rename(&backup, path) {
                return Err(PersistenceError::Recovery {
                    path: path.to_owned(),
                    backup,
                    source,
                });
            }
            return Err(failure);
        }
        if let Err(source) = fs::rename(&temporary, path) {
            let _ = fs::remove_file(&temporary);
            if let Err(restore_source) = fs::rename(&backup, path) {
                return Err(PersistenceError::Recovery {
                    path: path.to_owned(),
                    backup,
                    source: restore_source,
                });
            }
            sync_parent_directory(parent)?;
            return Err(PersistenceError::Io {
                path: path.to_owned(),
                source,
            });
        }
        sync_parent_directory(parent)?;
        if let Err(failure) = fs::remove_file(&backup) {
            warn!(path = %backup.display(), %failure, "could not remove completed write backup");
        } else {
            sync_parent_directory(parent)?;
        }
    } else if let Err(source) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(PersistenceError::Io {
            path: path.to_owned(),
            source,
        });
    } else {
        sync_parent_directory(parent)?;
    }
    Ok(())
}

fn write_json_temporary<T: Serialize>(temporary: &Path, value: &T) -> Result<()> {
    let mut file = fs::File::create(temporary).map_err(|source| PersistenceError::Io {
        path: temporary.to_owned(),
        source,
    })?;
    serde_json::to_writer_pretty(&mut file, value).map_err(|source| PersistenceError::Json {
        path: temporary.to_owned(),
        source,
    })?;
    file.write_all(b"\n")
        .map_err(|source| PersistenceError::Io {
            path: temporary.to_owned(),
            source,
        })?;
    file.sync_all().map_err(|source| PersistenceError::Io {
        path: temporary.to_owned(),
        source,
    })
}

#[cfg(unix)]
fn sync_parent_directory(parent: &Path) -> Result<()> {
    fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|source| PersistenceError::Io {
            path: parent.to_owned(),
            source,
        })
}

#[cfg(windows)]
fn sync_parent_directory(_parent: &Path) -> Result<()> {
    // FlushFileBuffers rejects directory handles on supported Windows versions.
    // The temporary file itself is flushed before Replace-by-rename.
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn sync_parent_directory(_parent: &Path) -> Result<()> {
    Ok(())
}

fn read_json_limited<T: DeserializeOwned>(path: &Path, limit: u64) -> Result<T> {
    let file = fs::File::open(path).map_err(|source| PersistenceError::Io {
        path: path.to_owned(),
        source,
    })?;
    let size = file
        .metadata()
        .map_err(|source| PersistenceError::Io {
            path: path.to_owned(),
            source,
        })?
        .len();
    if size > limit {
        return Err(PersistenceError::TooLarge {
            path: path.to_owned(),
            size,
            limit,
        });
    }
    serde_json::from_reader(file).map_err(|source| PersistenceError::Json {
        path: path.to_owned(),
        source,
    })
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let file = fs::File::open(path).map_err(|source| PersistenceError::Io {
        path: path.to_owned(),
        source,
    })?;
    serde_json::from_reader(file).map_err(|source| PersistenceError::Json {
        path: path.to_owned(),
        source,
    })
}

fn create_dir_all(path: &Path) -> Result<()> {
    fs::create_dir_all(path).map_err(|source| PersistenceError::Io {
        path: path.to_owned(),
        source,
    })
}

fn read_dir(path: &Path) -> Result<fs::ReadDir> {
    fs::read_dir(path).map_err(|source| PersistenceError::Io {
        path: path.to_owned(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use kfc_core::{
        CopyOptions, EndpointConfig, FilterConfig, JobStatus, RULE_SCHEMA_VERSION, TriggerConfig,
    };

    fn test_rule() -> CopyRule {
        CopyRule {
            schema_version: RULE_SCHEMA_VERSION,
            id: Uuid::new_v4(),
            name: "Persistence test".into(),
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
    fn rule_round_trip_and_atomic_update() {
        let root = std::env::temp_dir().join(format!("kfc-persistence-test-{}", Uuid::new_v4()));
        let repositories = FileRepositories::new(&root);
        let mut rule = test_rule();
        repositories.save_rule(&rule).unwrap();
        assert_eq!(repositories.load_rule(rule.id).unwrap(), rule);
        rule.name = "Updated".into();
        repositories.save_rule(&rule).unwrap();
        assert_eq!(repositories.list_rules().unwrap(), vec![rule]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn malformed_rule_does_not_hide_valid_rules() {
        let root = std::env::temp_dir().join(format!("kfc-persistence-test-{}", Uuid::new_v4()));
        let repositories = FileRepositories::new(&root);
        let rule = test_rule();
        repositories.save_rule(&rule).unwrap();
        fs::write(
            repositories.rules_dir().join("broken.json"),
            b"{ not json }",
        )
        .unwrap();

        assert_eq!(repositories.list_rules().unwrap(), vec![rule]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn malformed_job_does_not_hide_valid_job_summaries() {
        let root = std::env::temp_dir().join(format!("kfc-persistence-test-{}", Uuid::new_v4()));
        let repositories = FileRepositories::new(&root);
        let timestamp = Utc::now();
        let summary = JobSummary {
            job_id: Uuid::new_v4(),
            rule_id: Uuid::new_v4(),
            started_at: timestamp,
            finished_at: timestamp,
            status: JobStatus::Succeeded,
            copied_files: 1,
            skipped_files: 0,
            failed_files: 0,
            copied_bytes: 4,
            errors: Vec::new(),
        };
        repositories.save_job_summary(&summary).unwrap();
        let broken = repositories.jobs_dir().join("broken");
        fs::create_dir_all(&broken).unwrap();
        fs::write(broken.join("summary.json"), b"{ not json }").unwrap();

        assert_eq!(repositories.list_job_summaries().unwrap(), vec![summary]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn delete_job_removes_summary_and_events_and_is_idempotent() {
        let root = std::env::temp_dir().join(format!("kfc-persistence-test-{}", Uuid::new_v4()));
        let repositories = FileRepositories::new(&root);
        let job_id = Uuid::new_v4();
        let timestamp = Utc::now();
        let summary = JobSummary {
            job_id,
            rule_id: Uuid::new_v4(),
            started_at: timestamp,
            finished_at: timestamp,
            status: JobStatus::Succeeded,
            copied_files: 1,
            skipped_files: 0,
            failed_files: 0,
            copied_bytes: 4,
            errors: Vec::new(),
        };
        repositories.save_job_summary(&summary).unwrap();
        repositories.append_job_event(job_id, &summary).unwrap();
        assert_eq!(repositories.list_job_summaries().unwrap(), vec![summary]);

        repositories.delete_job(job_id).unwrap();
        assert!(repositories.list_job_summaries().unwrap().is_empty());

        // Deleting an already-removed job is a no-op, not an error.
        repositories.delete_job(job_id).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn clear_jobs_removes_every_job_and_is_idempotent() {
        let root = std::env::temp_dir().join(format!("kfc-persistence-test-{}", Uuid::new_v4()));
        let repositories = FileRepositories::new(&root);
        let timestamp = Utc::now();
        for _ in 0..3 {
            let summary = JobSummary {
                job_id: Uuid::new_v4(),
                rule_id: Uuid::new_v4(),
                started_at: timestamp,
                finished_at: timestamp,
                status: JobStatus::Succeeded,
                copied_files: 1,
                skipped_files: 0,
                failed_files: 0,
                copied_bytes: 4,
                errors: Vec::new(),
            };
            repositories.save_job_summary(&summary).unwrap();
        }
        assert_eq!(repositories.list_job_summaries().unwrap().len(), 3);

        repositories.clear_jobs().unwrap();
        assert!(repositories.list_job_summaries().unwrap().is_empty());

        // Clearing an already-empty history is a no-op, not an error.
        repositories.clear_jobs().unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn oversized_settings_are_rejected_before_parsing() {
        let root = std::env::temp_dir().join(format!("kfc-persistence-test-{}", Uuid::new_v4()));
        let repositories = FileRepositories::new(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("config.json"),
            vec![b' '; MAX_CONFIG_BYTES as usize + 1],
        )
        .unwrap();

        let failure = repositories
            .load_settings::<serde_json::Value>()
            .unwrap_err();
        assert!(matches!(failure, PersistenceError::TooLarge { .. }));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn serialization_failure_removes_temporary_file() {
        struct FailingValue;

        impl Serialize for FailingValue {
            fn serialize<S>(&self, _serializer: S) -> std::result::Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                Err(serde::ser::Error::custom("intentional failure"))
            }
        }

        let root = std::env::temp_dir().join(format!("kfc-persistence-test-{}", Uuid::new_v4()));
        let repositories = FileRepositories::new(&root);
        assert!(repositories.save_settings(&FailingValue).is_err());
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
        fs::remove_dir_all(root).unwrap();
    }
}
