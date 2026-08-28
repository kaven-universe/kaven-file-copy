use std::{
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
};

use chrono::{DateTime, Utc};
use kfc_core::{CopyRule, JobSummary};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use uuid::Uuid;

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
                rules.push(read_json(&entry.path())?);
            }
        }
        rules.sort_by_key(|rule| rule.name.to_lowercase());
        Ok(rules)
    }

    pub fn save_settings<T: Serialize>(&self, settings: &T) -> Result<()> {
        create_dir_all(&self.root)?;
        write_json_atomic(&self.root.join("settings.json"), settings)
    }

    pub fn load_settings<T: DeserializeOwned>(&self) -> Result<T> {
        read_json(&self.root.join("settings.json"))
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
                jobs.push(read_json(&summary)?);
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

    pub fn remove_jobs_older_than(&self, cutoff: DateTime<Utc>) -> Result<usize> {
        let jobs = self.list_job_summaries()?;
        let mut removed = 0;
        for job in jobs.into_iter().filter(|job| job.finished_at < cutoff) {
            let path = self.job_dir(job.job_id);
            fs::remove_dir_all(&path).map_err(|source| PersistenceError::Io { path, source })?;
            removed += 1;
        }
        Ok(removed)
    }

    fn rules_dir(&self) -> PathBuf {
        self.root.join("rules")
    }
    fn jobs_dir(&self) -> PathBuf {
        self.root.join("jobs")
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
    let temporary = path.with_extension("json.tmp");
    let mut file = fs::File::create(&temporary).map_err(|source| PersistenceError::Io {
        path: temporary.clone(),
        source,
    })?;
    serde_json::to_writer_pretty(&mut file, value).map_err(|source| PersistenceError::Json {
        path: temporary.clone(),
        source,
    })?;
    file.write_all(b"\n")
        .map_err(|source| PersistenceError::Io {
            path: temporary.clone(),
            source,
        })?;
    file.sync_all().map_err(|source| PersistenceError::Io {
        path: temporary.clone(),
        source,
    })?;
    if path.exists() {
        let backup = path.with_extension("json.backup");
        fs::rename(path, &backup).map_err(|source| PersistenceError::Io {
            path: path.to_owned(),
            source,
        })?;
        if let Err(source) = fs::rename(&temporary, path) {
            let _ = fs::rename(&backup, path);
            return Err(PersistenceError::Io {
                path: path.to_owned(),
                source,
            });
        }
        fs::remove_file(&backup).map_err(|source| PersistenceError::Io {
            path: backup,
            source,
        })?;
    } else {
        fs::rename(&temporary, path).map_err(|source| PersistenceError::Io {
            path: path.to_owned(),
            source,
        })?;
    }
    Ok(())
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
    use kfc_core::{CopyOptions, EndpointConfig, FilterConfig, RULE_SCHEMA_VERSION, TriggerConfig};

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
}
