use std::{
    path::Path,
    str::FromStr,
    sync::mpsc,
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use cron::Schedule;
use notify::{RecursiveMode, Watcher};

#[derive(Debug, thiserror::Error)]
pub enum SchedulerError {
    #[error("watcher error: {0}")]
    Watch(#[from] notify::Error),
    #[error("watcher channel disconnected")]
    Disconnected,
    #[error("invalid cron expression: {0}")]
    InvalidCron(String),
}

pub type Result<T> = std::result::Result<T, SchedulerError>;

pub fn next_cron_time(expression: &str, after: DateTime<Utc>) -> Result<Option<DateTime<Utc>>> {
    let schedule = Schedule::from_str(expression)
        .map_err(|error| SchedulerError::InvalidCron(error.to_string()))?;
    Ok(schedule.after(&after).next())
}

pub fn watch_debounced<F, S>(
    source: &Path,
    debounce: Duration,
    mut on_change: F,
    mut should_stop: S,
) -> Result<()>
where
    F: FnMut() -> bool,
    S: FnMut() -> bool,
{
    let (sender, receiver) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |event| {
        let _ = sender.send(event);
    })?;
    watcher.watch(source, RecursiveMode::Recursive)?;
    let poll_interval = Duration::from_millis(200);
    loop {
        let event = loop {
            if should_stop() {
                return Ok(());
            }
            match receiver.recv_timeout(poll_interval) {
                Ok(event) => break event,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(SchedulerError::Disconnected);
                }
            }
        };
        event?;
        let mut last = Instant::now();
        loop {
            if should_stop() {
                return Ok(());
            }
            let remaining = debounce.saturating_sub(last.elapsed());
            if remaining.is_zero() {
                break;
            }
            match receiver.recv_timeout(remaining.min(poll_interval)) {
                Ok(event) => {
                    event?;
                    last = Instant::now();
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(SchedulerError::Disconnected);
                }
            }
        }
        if !on_change() {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cron_has_a_next_occurrence() {
        let next = next_cron_time("0/10 * * * * * *", Utc::now()).unwrap();
        assert!(next.is_some());
    }

    #[test]
    fn watcher_can_stop_without_a_file_event() {
        let root = std::env::temp_dir().join(format!("kfc-watch-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        watch_debounced(
            &root,
            Duration::from_secs(1),
            || panic!("change callback should not run"),
            || true,
        )
        .unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
}
