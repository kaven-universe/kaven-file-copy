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

pub fn watch_debounced<F>(source: &Path, debounce: Duration, mut on_change: F) -> Result<()>
where
    F: FnMut() -> bool,
{
    let (sender, receiver) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |event| {
        let _ = sender.send(event);
    })?;
    watcher.watch(source, RecursiveMode::Recursive)?;
    loop {
        receiver
            .recv()
            .map_err(|_| SchedulerError::Disconnected)??;
        let mut last = Instant::now();
        loop {
            let remaining = debounce.saturating_sub(last.elapsed());
            match receiver.recv_timeout(remaining) {
                Ok(event) => {
                    event?;
                    last = Instant::now();
                }
                Err(mpsc::RecvTimeoutError::Timeout) => break,
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
}
