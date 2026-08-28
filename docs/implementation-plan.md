# Implementation Plan

This plan keeps every phase runnable and testable. The V1 critical path is to stabilize the rule format and shared copy engine before integrating the CLI, desktop UI, file watcher, and scheduler.

## 1. Definition of Done

V1 is complete when:

- The desktop application and CLI build and start on all three target platforms.
- The UI supports the complete rule lifecycle, including import, export, and execution.
- The CLI directly validates, previews, and executes rules exported by the UI.
- Manual, source-watch, and scheduled triggers all use the same copy engine.
- Failures, cancellation, retries, and abnormal exits produce explicit results and searchable logs.
- Core tests and the minimum cross-platform CI matrix pass.
- User documentation explains copy semantics, exclusions, background-execution limitations, and recovery behavior.

## 2. Milestones

### M0: Project Foundation and Decision Baseline

- Initialize the Rust workspace, Tauri 2, Vue 3/TypeScript/Vite, and baseline CI.
- Establish `core`, `local-endpoint`, file-based `persistence`, `scheduler`, `cli`, and `desktop` boundaries.
- Finalize the JSON Schema, error model, log fields, and versioning policy.
- Generate frontend contract types from Rust types or the JSON Schema and verify them in CI.
- Establish the minimal frontend baseline: Vue Composition API, small composables, regular CSS, and no global state, routing, HTTP, utility-CSS, or full component framework by default.
- Define atomic JSON writes, append-only JSON Lines logs, a rebuildable history index, retention, and process ownership locking.
- Add formatting, linting, unit-test, and three-platform build checks.

Exit criterion: an empty rule can be serialized, exported, and imported again; the CLI and desktop application call the same core version.

### M1: Local Copy Core

- Implement the local endpoint and capability description.
- Implement path-safety validation, scanning, and path/glob exclusions.
- Implement comparison, plan generation, and dry run.
- Implement streaming copy, temporary-file commit, concurrency control, retries, cancellation, and progress events.
- Add the job state machine and structured logging.

Priority tests: recursive destinations, path traversal, symlink cycles, source mutation during copy, inaccessible destinations, full disk, Unicode names, and large files.

Exit criterion: the core API alone reliably performs local incremental copy and returns a deterministic execution summary.

### M2: Usable CLI

- Add `rule validate`, `run`, `run --dry-run`, and `run --json`.
- Define stdout/stderr behavior and exit codes.
- Add cooperative cancellation with Ctrl+C.
- Document CLI usage and scripting examples.

Exit criterion: the CLI executes reference rules with results matching core integration tests, and automation can parse its JSON output reliably.

### M3: Desktop Rule Management and Manual Execution

- Build the rule list and editor.
- Add native source/destination pickers, source tree, and exclusion selection.
- Add glob editing and match preview.
- Add file-based rule persistence, import, export, atomic writes, and conflict handling.
- Add dry-run confirmation, progress, cancellation, summaries, and detailed logs.
- Add a CSS reset, design tokens, font fallbacks, and cross-platform visual checks.

Exit criterion: users can complete the rule lifecycle without the command line, and the CLI can execute UI-exported rules unchanged.

### M4: Change Detection and Scheduling

- Add the file watcher, debounce, overflow recovery, and full reconciliation scans.
- Add fixed-interval scheduling; add cron and timezone configuration if confirmed.
- Add per-rule mutual exclusion, repeated-trigger coalescing, and sleep/wake recovery.
- Show enable/disable state, last/next run time, and the application-exit limitation in the UI.

Exit criterion: high-frequency changes do not create a job storm, and missed events or system wake eventually produce the same result as a complete incremental scan.

### M5: Stabilization, Packaging, and Release Candidate

- Produce Windows, macOS, and Linux installers plus CLI binaries.
- Add log rotation, history cleanup, diagnostic export, and privacy redaction.
- Run large-tree performance, fault-injection, and long-running watcher tests.
- Complete the user guide, rule reference, exclusion syntax, troubleshooting, and upgrade documentation.
- Add code signing, automatic updates, and background execution as product decisions require.

Exit criterion: the release checklist passes, no known defect can corrupt data, and all known limitations are clearly presented.

## 3. Recommended Initial Backlog

1. Create the workspace and CI.
2. Define `CopyRuleV1`, JSON Schema, sample rules, and the migration interface.
3. Define file-based repository interfaces, atomic-write behavior, and the application-data layout.
4. Define `Endpoint`, its capability model, and a fake endpoint for testing.
5. Implement path safety and exclusion matching.
6. Implement scan, plan, and dry run.
7. Implement the executor, cancellation, retries, atomic commit, and progress events.
8. Add tracing logs and the job state machine.
9. Build CLI validate/run commands.
10. Build desktop rule management and directory selection.
11. Connect UI progress/logging and import/export.
12. Add watcher and scheduler support.
13. Complete platform-specific tests, packaging, and documentation.

## 4. Workflow and Quality Gates

Before merging a feature:

- Test the normal path and important failure paths.
- Pass `cargo fmt`, `cargo clippy`, Rust tests, TypeScript type checking, and frontend tests.
- Verify generated frontend contract types are current and Tauri IPC code uses them.
- Update documentation for changes to rule format, CLI output, or user behavior.
- Do not leak provider-specific types into core when adding endpoint capabilities.
- Perform a separate safety review for overwrite, deletion, credentials, or log privacy.

Recommended CI matrix:

| Check | Linux | Windows | macOS |
|---|:---:|:---:|:---:|
| Rust unit/integration tests | ✓ | ✓ | ✓ |
| Frontend types and tests | ✓ |  |  |
| Tauri build check | ✓ | ✓ | ✓ |
| Platform filesystem cases | ✓ | ✓ | ✓ |
| Installer/signing | Release pipeline | Release pipeline | Release pipeline |

## 5. Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Watcher loses or duplicates events | Inconsistency or job storms | Treat events as scan triggers; debounce, coalesce, and reconcile |
| Invalid source/destination relationship | Recursion or exhausted disk | Normalize and validate when saving and before execution |
| Cross-platform path/symlink differences | Inconsistency or root escape | Relative paths, capability interfaces, and platform tests |
| Interrupted copy leaves damaged files | Untrustworthy output | Temporary files, verification before commit, and orphan cleanup |
| UI and CLI diverge | Poor maintainability | One Rust core and contract tests across both entry points |
| Rust and frontend IPC types diverge | Runtime command failures | Generate TypeScript contracts and check generated output in CI |
| Platform WebViews render differently | Inconsistent desktop experience | CSS reset, design tokens, controlled fonts, and platform screenshot checks |
| Remote endpoint lacks local capabilities | Architectural rework | Capability negotiation; never assume atomic rename |
| Logs expose private paths | Privacy risk | Log levels, rotation, redaction, and no secrets in rules/logs |
| Large job history makes startup scans slow | Slow history view | Keep summaries compact, load lazily, and use a disposable rebuildable index only when measurements require it |

## 6. V2 Extension Validation

Before releasing V1, implement an in-memory or read-only test endpoint to verify that scan, plan, and execution do not directly depend on absolute local paths or `std::fs`. This is not a remote-transfer deliverable, but it validates the abstraction at low cost.

Expected V2 additions:

- Endpoint discovery and connection management.
- Credentials in the system keychain, with only credential IDs in rules.
- Chunking, resume, remote checksums, bandwidth limiting, and compression.
- Offline/reconnect state, idempotent commit, and persistent transfer queues.
- A formal conflict-detection model. Bidirectional synchronization must be designed as distinct product semantics rather than evolving implicitly from V1 one-way copy.

## 7. Product Decision Order

1. Before M0: application name, package identifier, launch-platform priority, and rule-file extension.
2. Before M3: import-conflict interaction, default log retention, and portable-mode support.
3. Before M4: whether jobs continue after the UI exits, whether cron is required, and the minimum scheduling interval.
4. Before M5: code signing, automatic updates, distribution channels, and telemetry policy.
