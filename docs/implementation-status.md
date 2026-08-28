# Implementation Status

Status date: 2026-08-28  
Version: 0.1.0 development build

## Current Assessment

The V1 design is not complete. The manual local-copy workflow and its shared Rust engine are usable, but several acceptance criteria in the design document remain partial or unimplemented. This development build is not yet a V1 release candidate.

## Working Now

- The complete desktop development application starts with `cd apps/desktop` followed by `npm run dev`. The internal `dev:web` script starts only Vite and does not provide Tauri IPC.
- The desktop UI can create, edit, duplicate, enable/disable, delete, import, export, preview, and manually execute rules. Duplicate import IDs require an overwrite-or-copy decision.
- Native source/destination dialogs, hierarchical path exclusions with child inclusion overrides, glob exclusions, job progress, cancellation, history, and basic event details are connected to Tauri commands.
- The local engine validates source/destination relationships, scans files and directories, applies exclusions, plans incremental work, bounds concurrency, retries failures, preserves modification times, and uses destination-side temporary files.
- Execution rejects lexical path traversal and existing destination links or junctions that resolve outside the destination root.
- Rule JSON is strictly deserialized according to required V1 fields, rejects unknown fields, and enforces the published concurrency and retry limits.
- File repositories persist rules, summaries, and JSON Lines events. A malformed rule or summary is skipped with a warning instead of hiding all healthy records.
- The CLI validates, previews, and executes exported rules; its end-to-end test performs a real copy and verifies exclusions. Watch cancellation is checked even when no file event arrives.
- The in-process desktop scheduler supports baseline watch debounce, fixed intervals, and UTC cron triggers while the application remains open.
- Deterministic application icons and generated platform assets are present.

## Verification Completed

- `cargo test --workspace` passes 23 Rust unit and integration tests, including destination-junction escape prevention, corrupt-record isolation, strict V1 rule deserialization, watcher cancellation, CLI exit codes, and real CLI validate/preview/copy behavior.
- `cargo clippy --workspace --all-targets -- -D warnings` passes.
- `cargo fmt --all -- --check` passes.
- `npm run build` passes strict TypeScript checking and the Vite production build.
- `npm run dev` launches Vite and the Tauri desktop process together on Windows.
- The repaired New Rule interaction was exercised in the rendered DOM and opens the rule editor without the previous `DataCloneError`.

## Partial or Missing Before V1

- The source tree is capped at depth 12 and 5,000 entries, is not virtualized, and does not show a dedicated exclusion match count.
- Job events do not yet contain every designed field such as trigger reason, scheduled time, duration, byte count, and complete error chains. Detail filtering, diagnostic redaction, size-based rotation, and configurable retention are missing.
- Watch overflow, sleep/wake recovery, unavailable-directory recovery, durable skipped-trigger records, and per-rule last/next-run display are missing. Cron timezone configuration is accepted by the model but not applied by the scheduler.
- Abnormal-exit recovery does not mark interrupted jobs, and desktop/background-process ownership locking is not implemented.
- UI automation, large-tree performance coverage, fault injection, and cross-platform screenshot tests are missing.
- macOS/Linux filesystem and packaging verification, signed installers, release automation, and optional updates are missing.
- TypeScript contracts are still maintained manually rather than generated from Rust or the Schema in CI.
- A settings UI, capacity-based retention, and optional content-hash comparison are not implemented.

Mirror deletion, bidirectional synchronization, credentials, and remote transfer remain intentionally out of scope for V1.

