# Kaven File Copy

Kaven File Copy is a lightweight cross-platform desktop application and CLI for safe, reusable local file and directory copying.

## Current Features

- Vue 3 + TypeScript desktop UI powered by Tauri 2.
- Shared Rust copy engine used by both the desktop application and CLI.
- Local file or directory sources and local directory destinations.
- Filterable list/tree relative-path exclusions, glob exclusions, and child inclusion overrides in the UI.
- Versioned JSON rules with import/export and JSON Schema.
- Incremental planning based on size and modification time, plus dry-run preview with live progress, filtering, and list/tree result views.
- Bounded concurrent copying, retries, cooperative cancellation, and modification-time preservation.
- Temporary-file commit with destination backup/restore during replacement.
- Manual, debounced filesystem-watch, fixed-interval, and cron triggers.
- JSON rule/settings/job storage and per-job JSON Lines event logs; no database dependency.
- Recursive-destination, path-escape, and symlink-following safeguards.

V1 synchronization is a safe one-way incremental copy. It does not delete extra destination files or provide bidirectional synchronization.

## Repository Layout

```text
apps/desktop/          Vue UI and Tauri application
crates/core/           rules, validation, filtering, plans, and job models
crates/local-endpoint/ local scan and copy engine
crates/persistence/    JSON and JSON Lines repositories
crates/scheduler/      watcher debounce and cron calculation
crates/cli/            kfc command-line application
schemas/               versioned exported-rule schema
docs/                  requirements, architecture, and implementation plan
```

## Prerequisites

- Rust toolchain with Cargo
- Node.js and npm
- Platform prerequisites required by Tauri 2

## Build and Test

```text
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings

cd apps/desktop
npm install
npm run build
npm run tauri -- build --debug --no-bundle
```

The desktop executable is generated under `target/debug/` for a debug build.

## Run the Desktop Application

```text
cd apps/desktop
npm run dev
```

Automatic and scheduled rules run while the desktop application is open. V1 does not install a permanent operating-system service.

`npm run dev` starts both Vite and the Tauri/Rust process. `npm run dev:web` is only for frontend development in a browser, where native dialogs, persistence, and copy commands are unavailable. The equivalent explicit Tauri command is `npm run tauri -- dev`; running `npm run tauri` without a subcommand only prints the Tauri CLI help.

If port `5173` is reported as already in use, stop any separately started `dev:web` or stale Vite process before launching the desktop application.

## CLI

```text
cargo run -p kfc-cli -- rule validate path/to/rule.kfc.json
cargo run -p kfc-cli -- run path/to/rule.kfc.json --dry-run
cargo run -p kfc-cli -- run path/to/rule.kfc.json
cargo run -p kfc-cli -- watch path/to/rule.kfc.json
cargo run -p kfc-cli -- schedule path/to/rule.kfc.json
```

Add `--json` to validation, run, watch, or schedule commands for machine-readable stdout. Human-readable diagnostics are written to stderr.

## Documentation

- [Requirements and Architecture Design](docs/requirements-and-design.md)
- [Implementation Plan](docs/implementation-plan.md)
- [Current Implementation Status](docs/implementation-status.md)
- [Copy Rule V1 JSON Schema](schemas/copy-rule-v1.schema.json)
