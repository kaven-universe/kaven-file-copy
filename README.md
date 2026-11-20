# Kaven File Copy

Kaven File Copy is a lightweight cross-platform desktop application and CLI for safe, reusable local file and directory copying.

The project is under active pre-release development. Windows 10 and later is
the release-validated desktop target. The Rust core and Tauri application are
designed for Linux and macOS as well, but those packages are not yet part of
the release gate.

## Current Features

- Vue 3 + TypeScript desktop UI powered by Tauri 2.
- Shared Rust copy engine used by both the desktop application and CLI.
- Local file or directory sources and local directory destinations.
- Positive-selection source tree and list views: check the files and folders to copy, with tri-state child deselection.
- A version rule: extract versions from top-level file names with one regex capture group, then select whole versions to copy.
- Rename destination files from source names with regex capture groups (for example extract the version and rebuild the file name).
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

- Stable Rust with Cargo (minimum Rust 1.88, matching locked dependencies)
- Node.js 24 and npm 11
- Platform prerequisites required by Tauri 2

## Build and Test

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

Set-Location apps/desktop
npm ci
npm run lint
npm run typecheck
npm test
npm run build
npm run tauri -- build --debug --no-bundle
```

`npm run verify` runs the frontend lint, type-check, test, and production-build
gates together. Dependency security checks are `cargo audit` (when installed)
and `npm audit`; high-severity findings must be fixed or recorded as a dated
risk acceptance before release. Tag-triggered release automation runs the same
Rust and frontend gates before creating Windows artifacts. Gitea retains the
complete development history, while GitHub exposes only the current parentless
`release` snapshot and hosts signed Release assets; see
[`docs/RELEASE.md`](docs/RELEASE.md).

The desktop executable is generated under `target/debug/` for a debug build.

## Run the Desktop Application

```text
cd apps/desktop
npm run dev
```

Automatic and scheduled rules run while the desktop application is open. V1 does not install a permanent operating-system service.

Signed desktop updates are announced through
`https://public.data.kaven.xyz/json/kaven-file-copy/latest.json`. The application
checks this channel at startup and from **Help > Check for updates**.

Application settings, copy rules, and task history are stored under the native
local-data base at `Kaven/Data/Kaven-File-Copy/`: settings in `config.json`, and
rules/history under `data/`. Task history is retained until it is removed
explicitly from the History view; startup never prunes it by age or size.

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
- [Architecture decisions](docs/decisions/)

## License

Kaven File Copy is licensed under the [Apache License 2.0](LICENSE), as
recorded in [ADR 0003](docs/decisions/0003-adopt-apache-2-license.md).
