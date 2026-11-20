use std::{
    fs::File,
    io::{self, Write},
    path::{Path, PathBuf},
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use kfc_core::{CopyRule, JobStatus, TriggerConfig};
use kfc_local_endpoint::LocalCopyEngine;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(name = "kfc", version, about = "Run Kaven File Copy rules")]
struct Args {
    #[arg(long, global = true)]
    verbose: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Rule {
        #[command(subcommand)]
        command: RuleCommand,
    },
    Run {
        rule: PathBuf,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        json: bool,
    },
    Watch {
        rule: PathBuf,
        #[arg(long)]
        json: bool,
    },
    Schedule {
        rule: PathBuf,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum RuleCommand {
    Validate {
        rule: PathBuf,
        #[arg(long)]
        json: bool,
    },
}

fn main() -> ExitCode {
    let args = Args::parse();
    init_logging(args.verbose);
    match dispatch(args) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::from(error_exit_code(&error))
        }
    }
}

fn dispatch(args: Args) -> Result<u8> {
    match args.command {
        Command::Rule {
            command: RuleCommand::Validate { rule, json },
        } => {
            let rule = load_rule(&rule)?;
            LocalCopyEngine.validate_rule(&rule)?;
            if let TriggerConfig::Cron { expression, .. } = &rule.trigger {
                kfc_scheduler::next_cron_time(expression, chrono::Utc::now())?;
            }
            if json {
                println!("{}", serde_json::json!({"valid": true, "ruleId": rule.id}));
            } else {
                println!("Rule '{}' is valid.", rule.name);
            }
            Ok(0)
        }
        Command::Run {
            rule,
            dry_run,
            json,
        } => run_once(&rule, dry_run, json),
        Command::Watch { rule, json } => watch(&rule, json),
        Command::Schedule { rule, json } => schedule(&rule, json),
    }
}

fn load_rule(path: &Path) -> Result<CopyRule> {
    let file =
        File::open(path).with_context(|| format!("failed to open rule {}", path.display()))?;
    serde_json::from_reader(file)
        .with_context(|| format!("failed to parse rule {}", path.display()))
}

fn run_once(path: &Path, dry_run: bool, json: bool) -> Result<u8> {
    let cancelled = install_cancel_handler()?;
    run_with_cancel(path, dry_run, json, &cancelled)
}

fn run_with_cancel(path: &Path, dry_run: bool, json: bool, cancelled: &AtomicBool) -> Result<u8> {
    let rule = load_rule(path)?;
    let engine = LocalCopyEngine;
    let plan = engine.plan(&rule)?;
    if dry_run {
        if json {
            println!("{}", serde_json::to_string(&plan)?);
        } else {
            println!("Rule: {}", rule.name);
            println!("Files to copy: {}", plan.stats.files_to_copy);
            println!("Bytes to copy: {}", plan.stats.bytes_to_copy);
            println!("Files to skip: {}", plan.stats.files_to_skip);
            println!("Warnings: {}", plan.stats.warnings);
            for item in &plan.items {
                println!(
                    "{:?}\t{}\t{}",
                    item.action,
                    item.relative_path.display(),
                    item.reason
                );
            }
        }
        return Ok(0);
    }
    let summary = engine.execute(&rule, &plan, cancelled, |event| {
        if json {
            println!("{}", serde_json::to_string(event).unwrap_or_default());
        } else {
            eprint!(
                "\r{}/{} files, {}/{} bytes",
                event.completed_files, event.total_files, event.copied_bytes, event.total_bytes
            );
            let _ = io::stderr().flush();
        }
    })?;
    if !json {
        eprintln!();
        println!(
            "Status: {:?}; copied: {}; failed: {}",
            summary.status, summary.copied_files, summary.failed_files
        );
    } else {
        println!("{}", serde_json::to_string(&summary)?);
    }
    Ok(exit_for_status(&summary.status))
}

fn watch(path: &Path, json: bool) -> Result<u8> {
    let rule = load_rule(path)?;
    let debounce = match rule.trigger {
        TriggerConfig::Watch { debounce_ms } => Duration::from_millis(debounce_ms),
        _ => Duration::from_secs(1),
    };
    let source = rule.source.path().clone();
    let cancelled = install_cancel_handler()?;
    eprintln!("Watching {}", source.display());
    kfc_scheduler::watch_debounced(
        &source,
        debounce,
        || {
            if cancelled.load(Ordering::Relaxed) {
                return false;
            }
            if let Err(error) = run_with_cancel(path, false, json, &cancelled) {
                eprintln!("synchronization failed: {error:#}");
            }
            true
        },
        || cancelled.load(Ordering::Relaxed),
    )?;
    Ok(4)
}

fn schedule(path: &Path, json: bool) -> Result<u8> {
    let rule = load_rule(path)?;
    let cancelled = install_cancel_handler()?;
    match rule.trigger {
        TriggerConfig::Interval { seconds } if seconds > 0 => {
            while !cancelled.load(Ordering::Relaxed) {
                wait_cancelable(&cancelled, Duration::from_secs(seconds));
                if !cancelled.load(Ordering::Relaxed) {
                    let _ = run_with_cancel(path, false, json, &cancelled)?;
                }
            }
        }
        TriggerConfig::Cron { expression, .. } => {
            while !cancelled.load(Ordering::Relaxed) {
                let next = kfc_scheduler::next_cron_time(&expression, chrono::Utc::now())?
                    .context("cron expression has no next occurrence")?;
                let wait = (next - chrono::Utc::now()).to_std().unwrap_or_default();
                wait_cancelable(&cancelled, wait);
                if !cancelled.load(Ordering::Relaxed) {
                    let _ = run_with_cancel(path, false, json, &cancelled)?;
                }
            }
        }
        _ => anyhow::bail!("rule trigger must be interval or cron for schedule mode"),
    }
    Ok(4)
}

fn install_cancel_handler() -> Result<Arc<AtomicBool>> {
    let cancelled = Arc::new(AtomicBool::new(false));
    let signal = cancelled.clone();
    ctrlc::set_handler(move || {
        signal.store(true, Ordering::Relaxed);
    })?;
    Ok(cancelled)
}

fn exit_for_status(status: &JobStatus) -> u8 {
    match status {
        JobStatus::Succeeded => 0,
        JobStatus::Partial => 1,
        JobStatus::Failed => 3,
        JobStatus::Cancelled => 4,
    }
}

fn error_exit_code(failure: &anyhow::Error) -> u8 {
    if failure.downcast_ref::<serde_json::Error>().is_some() {
        return 2;
    }
    if failure.downcast_ref::<std::io::Error>().is_some() {
        return 3;
    }
    if let Some(failure) = failure.downcast_ref::<kfc_core::KfcError>() {
        return match failure {
            kfc_core::KfcError::InvalidRule(_)
            | kfc_core::KfcError::UnsupportedSchema { .. }
            | kfc_core::KfcError::PathEscape(_)
            | kfc_core::KfcError::UnsafeRelationship(_) => 2,
            kfc_core::KfcError::Io { .. } => 3,
            kfc_core::KfcError::Cancelled => 4,
        };
    }
    10
}

fn wait_cancelable(cancelled: &AtomicBool, duration: Duration) {
    let deadline = std::time::Instant::now() + duration;
    while !cancelled.load(Ordering::Relaxed) {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        thread::sleep(remaining.min(Duration::from_millis(200)));
    }
}

fn init_logging(verbose: bool) {
    let default = if verbose { "kfc=debug" } else { "kfc=info" };
    let _ = tracing_subscriber::fmt()
        .with_writer(io::stderr)
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default)),
        )
        .try_init();
}
