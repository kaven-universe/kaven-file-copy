# Implementation Status

Status date: 2026-11-19

Version: 0.1.3 development build

## Current Assessment

The V1 design is not complete. The manual local-copy workflow and its shared Rust engine are usable, but several acceptance criteria in the design document remain partial or unimplemented. This development build is not yet a V1 release candidate.

## Working Now

- The complete desktop development application starts with `cd apps/desktop` followed by `npm run dev`. The internal `dev:web` script starts only Vite and does not provide Tauri IPC.
- The desktop UI can create, edit, duplicate, enable/disable, delete, import, export, preview, and manually execute rules. Duplicate import IDs require an overwrite-or-copy decision.
- Native source/destination dialogs, positive-selection file picker with tri-state deselection overrides, job progress, cancellation, history, and basic event details are connected to Tauri commands.
- The local engine validates source/destination relationships, scans files and directories, applies the file selection, plans incremental work, bounds concurrency, retries failures, preserves modification times, and uses destination-side temporary files.
- Execution rejects lexical path traversal and existing destination links or junctions that resolve outside the destination root.
- Rule JSON is strictly deserialized according to required V1 fields, rejects unknown fields, and enforces the published concurrency and retry limits.
- File repositories persist rules, summaries, and JSON Lines events. A malformed rule or summary is skipped with a warning instead of hiding all healthy records.
- The CLI validates, previews, and executes exported rules; its end-to-end test performs a real copy and verifies the selection. Watch cancellation is checked even when no file event arrives.
- The in-process desktop scheduler supports baseline watch debounce, fixed intervals, and UTC cron triggers while the application remains open.
- Deterministic application icons and generated platform assets are present.
- Locale and recent-path preferences use the backend-owned, versioned `config.json`; the frontend does not use persistent browser storage.
- Runtime files use the fixed platform-local `Kaven/Data/Kaven-File-Copy` root, with rules and task history under `data/` and WebView state under `webview/`.
- Task history is retained until the user explicitly removes records; startup does not prune history.
- Signed Windows updates use the canonical `https://public.data.kaven.xyz/json/kaven-file-copy/latest.json` manifest. The desktop checks automatically at startup and exposes manual check/install actions from the Help menu.

## Verification Completed

- `cargo test --workspace` passes 50 Rust unit and integration tests, including update redirect-boundary enforcement, destination-junction escape prevention, corrupt-record isolation, settings-version rejection, stable runtime-root resolution, watcher cancellation, CLI exit codes, and real CLI validate/preview/copy behavior.
- `cargo clippy --workspace --all-targets -- -D warnings` passes.
- `cargo fmt --all -- --check` passes.
- `npm run lint`, `npm run typecheck`, `npm test` (28 tests), and `npm run build` pass.
- `npm run dev` launches Vite and the Tauri desktop process together on Windows.
- The repaired New Rule interaction was exercised in the rendered DOM and opens the rule editor without the previous `DataCloneError`.

## Partial or Missing Before V1

- The source tree is capped at depth 12 and 5,000 entries, is not virtualized, and does not show a dedicated selection match count.
- Job events do not yet contain every designed field such as trigger reason, scheduled time, duration, byte count, and complete error chains. Detail filtering, diagnostic redaction, and separate bounded diagnostic-log rotation are missing.
- Watch overflow, sleep/wake recovery, unavailable-directory recovery, durable skipped-trigger records, and per-rule last/next-run display are missing. Cron timezone configuration is accepted by the model but not applied by the scheduler.
- Abnormal-exit recovery does not mark interrupted jobs, and desktop/background-process ownership locking is not implemented.
- UI automation, large-tree performance coverage, fault injection, and cross-platform screenshot tests are missing.
- macOS/Linux filesystem and packaging verification remains missing. Signed Windows updater artifacts and the canonical update channel are configured; tag-triggered release automation is gated by format, lint, type-check, test, audit, and build verification.
- TypeScript contracts are still maintained manually rather than generated from Rust or the Schema in CI.
- A broader settings UI, history storage-usage reporting, and optional content-hash comparison are not implemented.

Mirror deletion, bidirectional synchronization, credentials, and remote transfer remain intentionally out of scope for V1.

