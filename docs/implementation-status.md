# Implementation Status

Status date: 2026-08-28  
Version: 0.1.0 development build

## Implemented

- Rust workspace with shared domain models, validation, exclusion matching, copy plans, job summaries, and endpoint contracts.
- Local endpoint capability description, metadata/list/read operations, transactional temporary writes, and atomic-style commit with backup restoration.
- Local file and directory scanning with path and glob exclusions, hidden-file handling, symlink skipping, and recursive-destination protection.
- Incremental comparison by size and modification time, dry-run plans, bounded concurrency, retries, cancellation, source-change detection, and modification-time preservation.
- Versioned rule JSON and Copy Rule V1 JSON Schema.
- File-based repositories for rules, settings, job summaries, and JSON Lines job events, including atomic JSON replacement and 30-day startup cleanup.
- CLI commands for validation, dry run, execution, source watching, interval scheduling, and cron scheduling, with machine-readable output and documented exit codes.
- Vue 3/TypeScript/Vite UI with rule creation/editing/duplication/deletion, native source/destination selection, source-tree exclusions, glob exclusions, import/export, dry-run preview, manual execution, progress, cancellation, job history, and detailed logs.
- In-process desktop scheduler with dynamic rule reload, watcher registration and retry, debounce, interval/cron triggers, and shared single-job ownership.
- Deterministic SVG application icon and generated platform icon assets.

## Verification Completed

- `cargo test --workspace`: passed, including real CLI validate/dry-run/copy tests.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `npm run build`: strict TypeScript checking and Vite production build passed.
- `npm run tauri -- build --debug --no-bundle`: Windows desktop executable built successfully.
- Current frontend output is approximately 79 KB JavaScript, or 31 KB gzip, excluding the system WebView.

## Remaining Before a V1 Release

- Run filesystem and packaging tests on macOS and Linux in CI; only the Windows executable has been built locally so far.
- Add signed release installers, release automation, and optional automatic updates.
- Add UI automation and cross-platform screenshot comparison.
- Add total-capacity-based log retention and a settings UI; age-based cleanup is implemented.
- Add an optional content-hash comparison mode if required for V1.
- Add deeper platform tests for Windows long paths/file locks and Unix permissions/symbolic links.
- Generate TypeScript contracts automatically from Rust/schema in CI; the current typed mirror is checked by TypeScript but is still maintained in the repository.
- Decide whether a system tray/background process and launch at startup belong in V1. Automatic jobs currently run only while the desktop application or CLI process is active.

Mirror deletion, bidirectional synchronization, credentials, and remote transfer remain intentionally out of scope for V1.

