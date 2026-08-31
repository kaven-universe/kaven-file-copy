# File Copy Application: Requirements and Architecture Design

Document status: Draft v0.1  
Target release: V1 (local copy)  
Target platforms: Windows, macOS, and Linux

## 1. Background and Goals

Build a cross-platform file copy application with both a desktop UI and a CLI. Users can create reusable copy rules, choose the files or directories to copy, run jobs manually, automatically, or on a schedule, and inspect detailed execution records.

V1 transfers files only on the local machine. The core model must not depend on local paths so that LAN, SSH/SFTP, object storage, or a custom device protocol can be added later without redesigning the copy engine.

## 2. Scope

### 2.1 Required in V1

1. Create, edit, duplicate, enable, disable, and delete copy rules.
2. Select source files/directories and destination directories with native system pickers.
3. Browse the source tree in the UI and check the files and folders to copy, with tri-state child deselection.
4. Persist rules and support exporting and importing them.
5. Provide a CLI that validates, previews, and executes rules exported by the UI.
6. Support manual copy, automatic synchronization after detecting source changes, and interval- or cron-based scheduled synchronization.
7. Support incremental copy, job cancellation, progress reporting, and retry on failure.
8. Record detailed rule, job, file-operation, error, and statistics logs.
9. Keep behavior consistent across Windows, macOS, and Linux while handling platform-specific paths correctly.

### 2.2 Explicitly Out of Scope for V1

- Cross-device transfer, cloud accounts, and a central server.
- Automatic conflict merging after concurrent changes on multiple devices.
- Block-level delta transfer, compression, and end-to-end encryption.
- A permanent operating-system service. V1 automatic and scheduled jobs run only while the desktop application or a separate background process is running.
- Deleting extra files at the destination by default. V1 uses safe one-way incremental copy; deleting a source file does not delete the destination copy.

### 2.3 Recommended Follow-up Releases

- V1.1: optional mirror mode, system tray support, launch at startup, job-history search, and log archiving.
- V2: remote endpoints, authentication and credential management, resumable transfer, capability negotiation, conflict policies, and transport encryption.

## 3. Terminology and Behavior

- **Copy rule (Rule):** reusable configuration containing the source, destination, file selection, comparison policy, and trigger policy.
- **One-way incremental copy:** scan the source and copy only files that do not exist at the destination or are considered changed.
- **Automatic synchronization:** watch the source, coalesce repeated events, and initiate an incremental scan. Watch events are only triggers; scan results remain authoritative to avoid missed changes.
- **Scheduled synchronization:** initiate an incremental scan at a fixed interval or according to a cron expression.
- **Selection:** a normalized path relative to the source root. Selecting a directory selects its descendants; the deepest matching path rule wins. An empty selection copies nothing.
- **Deselection override:** a normalized relative path that un-selects a child of a selected path.
- **Dry run:** generate an execution plan and statistics without modifying the destination.

In V1, “synchronization” means safe one-way incremental copy. It does not mean bidirectional synchronization or mirror deletion.

## 4. User Stories and Acceptance Criteria

### 4.1 Rule Management

As a user, I can select a source and destination, choose the files to copy, and save the rule for repeated use.

Acceptance criteria:

- The source may be a file or directory; the destination must be a directory.
- The source and destination must exist and be accessible, and they cannot be the same directory.
- The destination cannot be inside the source directory because that would cause recursive copying.
- A rule name is required. Rule IDs use UUIDs and do not depend on names.
- The UI stores the selection and its deselection overrides as paths relative to the source root and previews their matches.
- Editing a rule does not silently alter a running job. A job uses an immutable rule snapshot captured at startup.

### 4.2 Export, Import, and CLI

As a user, I can export a rule, import it elsewhere, or execute it with the CLI.

Acceptance criteria:

- The export format is UTF-8 JSON with a `schemaVersion` field.
- Import performs structural, version, path, and safety validation. A duplicate rule ID prompts the user to overwrite or save as a new rule.
- Exported files never contain passwords or tokens. Future remote rules store only credential references.
- The CLI supports at least:

```text
kfc rule validate <rule.json>
kfc run <rule.json> [--dry-run] [--json] [--verbose]
kfc watch <rule.json>
kfc schedule <rule.json>
```

- CLI exit codes distinguish success, partial failure, invalid rules, and execution errors. Machine-readable JSON output is available.

### 4.3 Copy Execution

As a user, I can preview and execute a copy job, monitor its progress, and cancel it when necessary.

Acceptance criteria:

- The application generates a plan before executing file operations.
- The default comparison uses file size and modification time; optional content hashing provides higher-confidence verification.
- Files are written to temporary files in the destination directory and atomically replaced after completion, preventing incomplete files from appearing valid.
- Modification times are preserved where possible. Cross-platform permission, ownership, extended-attribute, and symbolic-link behavior follows explicit policies and produces warnings when unsupported.
- A single-file failure does not immediately abort the entire batch. Final job status can be `succeeded`, `partial`, `failed`, or `cancelled`.
- The same rule does not run concurrently by default. Repeated triggers are coalesced into one pending job.

### 4.4 Automatic and Scheduled Synchronization

As a user, I can enable source change detection or configure a schedule.

Acceptance criteria:

- Automatic synchronization uses debouncing, for example running one second after the last change event.
- A full incremental scan runs after watcher overflow, system wake, or recovery from an unavailable directory.
- Scheduling supports fixed intervals. Cron is an enhancement with defined timezone and daylight-saving behavior.
- The scheduler records the trigger reason, scheduled time, actual start time, and reason for any skipped run.
- The UI clearly states that automatic and scheduled jobs stop when the application exits. If a background process is provided, it and the UI share a single scheduler ownership lock.

### 4.5 Logging

As a user or maintainer, I can determine what a job copied, what it skipped, and why an operation failed.

Acceptance criteria:

- Structured logs include at least timestamp, level, job ID, rule ID, trigger reason, phase, source/destination relative path, operation, result, duration, byte count, and error chain.
- The UI shows summaries and filterable details. The CLI supports human-readable output and JSON Lines.
- Logs are stored in the platform application-data directory and support rotation by size/date and a configurable retention policy.
- Paths may contain private information. Diagnostic export supports path redaction.

## 5. Non-functional Requirements

### 5.1 Safety and Reliability

- Prevent recursive source/destination relationships, path traversal, and symbolic-link escapes.
- Match the selection only against normalized relative paths and reject any `..` path that escapes the source root.
- Do not follow directory symbolic links by default. The policy is `skip`, `copy-link`, or explicit `follow`; `follow` must detect cycles.
- Detect source metadata changes between the start and end of a copy. Mark unstable files and retry them a limited number of times.
- Cancellation is cooperative: stop scheduling new files, clean up temporary files from the current job, and keep completed files.

### 5.2 Performance

- Scanning and copying are streamed; file contents are never loaded into memory in full.
- Large directory trees use virtualized UI lists. Progress events are rate-limited to avoid saturating IPC with per-file updates.
- Copy concurrency is configurable and conservative by default. Future endpoint types may provide different recommended settings.
- Hashes are calculated only when requested. The default fast comparison does not read complete file contents.

### 5.3 Portability and Testability

- The copy engine, rule validation, and scheduler logic do not depend on the UI.
- Platform tests cover path case sensitivity, invalid characters, long paths, permissions, and file locking.
- Filesystem and clock access are injected through interfaces so unit tests can use temporary directories and virtual time.

## 6. Technology Selection

Recommended stack:

- **Desktop shell:** Tauri 2.
- **UI:** Vue 3, TypeScript, and Vite, using Vue's Composition API.
- **Core and CLI:** a Rust workspace.
- **Persistence:** versioned JSON files for rules, settings, and job summaries; JSON Lines for detailed job events. V1 does not require SQLite.
- **File watching:** the Rust `notify` ecosystem, with watch events used only to trigger reconciliation scans.
- **Logging:** the `tracing` ecosystem, shared by the desktop application and CLI.
- **Serialization:** Serde and JSON Schema, followed by semantic validation during import.

A shared Rust core guarantees identical copy semantics in the UI and CLI. Tauri has a smaller distribution footprint than solutions that bundle a complete browser runtime and integrates well with native file pickers and operating-system capabilities.

TypeScript remains part of the lightweight frontend because it validates Tauri command parameters, results, rule fields, and job states. Shared API types should be generated from Rust types or the versioned JSON Schema where practical, rather than maintained independently.

V1 deliberately avoids Pinia, Vue Router, Axios, Tailwind CSS, and full UI component suites such as Element Plus or Vuetify. Component-local state, small composables, native `fetch` or Tauri commands, and regular CSS with design tokens are sufficient initially. A dependency is added only when a measured requirement cannot be met cleanly with this baseline.

The main trade-off is that Tauri uses the operating system WebView, so pixel-level rendering cannot be absolutely identical across platforms. A CSS reset, explicit design tokens, a controlled font fallback list, and cross-platform screenshot checks keep the experience consistent. The UI must not depend on platform-specific WebView behavior.

## 7. Overall Architecture

```text
┌─────────────────────┐       ┌─────────────────────┐
│ Desktop UI (Vue 3)  │       │ CLI (kfc)           │
└──────────┬──────────┘       └──────────┬──────────┘
           │ Tauri commands/events                  │
           └──────────────┬─────────────────────────┘
                          ▼
               ┌─────────────────────┐
               │ Application Service │
               │ rules/jobs/schedule │
               └──────────┬──────────┘
                          ▼
               ┌─────────────────────┐
               │ Copy/Sync Core      │
               │ scan→plan→execute   │
               └──────┬────────┬─────┘
                      │        │
              ┌───────▼───┐ ┌──▼────────────┐
              │ Endpoint  │ │ Rule/Log/Job  │
              │ adapters  │ │ repositories  │
              └───────┬───┘ └───────────────┘
                      ▼
             Local filesystem (V1)
             Remote endpoints (future)
```

### 7.1 Proposed Workspace Layout

```text
apps/
  desktop/           Vue 3 UI and Tauri commands
crates/
  core/              domain models, validation, scan, plan, execution
  local-endpoint/    local filesystem adapter
  scheduler/         watcher, debounce, interval/cron
  persistence/       rule, job-history, and log storage
  cli/               kfc command-line entry point
schemas/
  copy-rule-v1.schema.json
docs/
```

Dependencies point inward only: desktop, CLI, and infrastructure depend on core; core never depends on Tauri, Vue, or a remote-provider SDK.

### 7.2 Frontend Structure

```text
apps/desktop/src/
  components/
    RuleEditor.vue
    SourceTree.vue
    JobProgress.vue
    LogViewer.vue
  composables/
    useRules.ts
    useJobs.ts
    useSettings.ts
  api/
    tauri.ts
  types/
    generated.ts
  styles/
    tokens.css
    base.css
  App.vue
```

- Vue Single-File Components own presentation and short-lived local state.
- Composables contain shared application behavior without introducing a global state library.
- `api/tauri.ts` is the only frontend boundary for Tauri commands and events.
- `types/generated.ts` is generated from the Rust/JSON contract and is not edited by hand.
- Navigation starts as simple application state. Vue Router is introduced only if deep linking or sufficiently complex navigation becomes a real requirement.

## 8. Core Domain Model

### 8.1 Rule Example

```json
{
  "schemaVersion": 1,
  "id": "018f6f2c-8f14-7b7a-8a04-8e7b1fe21c33",
  "name": "Photo backup",
  "enabled": true,
  "source": { "type": "local", "path": "D:/Photos" },
  "destination": { "type": "local", "path": "E:/Backup/Photos" },
  "filters": {
    "include": ["docs", "src/main.rs"],
    "exclude": ["docs/drafts"],
    "includeHidden": true
  },
  "copy": {
    "mode": "incremental",
    "compare": "size-and-mtime",
    "overwrite": "if-changed",
    "symlinks": "skip",
    "preserveMtime": true,
    "maxConcurrency": 4,
    "retryCount": 2
  },
  "trigger": { "type": "manual" }
}
```

Paths are endpoint-specific configuration. The core interacts through the `Endpoint` interface and does not parse remote protocol details. A future version may add environment variables or named path mappings for portability; V1 performs no implicit path substitution.

### 8.2 Endpoint Abstraction

The abstraction must not assume that every backend supports all local-filesystem semantics. Capabilities should be explicit:

```text
Endpoint
  stat(relativePath)
  list(relativeDirectory, cursor)
  openRead(relativePath, offset?)
  openWriteTemp(relativePath)
  commitTemp(tempHandle, relativePath)
  createDirectory(relativePath)
  setMetadata(relativePath, supportedMetadata)
  capabilities() -> atomicRename, randomRead, symlink, checksum, ...
```

The executor selects a strategy based on endpoint capabilities. Future remote protocols can implement the same interface and access secrets through credential references stored in the system keychain; the rule model does not need to be replaced.

### 8.3 Job State Machine

```text
queued → scanning → planning → running → succeeded
   │         │          │          ├────→ partial
   │         │          │          ├────→ failed
   └─────────┴──────────┴──────────┴────→ cancelled
```

Each job stores an immutable rule snapshot, trigger reason, and statistics. After an abnormal application exit, any running job is marked `interrupted`. The next run performs a new scan instead of blindly resuming an obsolete plan.

## 9. Scan, Match, and Copy Flow

1. Validate the rule and endpoint availability.
2. Normalize source and destination roots and check for recursion or root escape.
3. Stream a depth-first or breadth-first scan of the source.
4. Apply the selection before entering a directory, but continue toward any deeper selection override.
5. Query destination metadata and generate `CreateDir`, `CopyFile`, `Skip`, or `Warn` operations according to the comparison policy.
6. Stop here for a dry run; otherwise execute the plan in dependency order.
7. Write each file to a same-directory temporary file, verify its size and optional hash, and commit it.
8. Persist aggregate success, skipped, failed, byte-count, and duration statistics.

Selection paths always use `/` as the separator. Platform paths are converted at the adapter boundary. An `include` path selects that relative path and its descendants. A more-specific `exclude` path deselects part of that subtree again, and an even deeper `include` can re-select a part of the deselected subtree. An empty `include` list copies nothing, so the UI presents "check the files you want to copy". The UI represents mixed subtrees with an indeterminate parent checkbox.

## 10. UI Information Architecture

### 10.1 Rule List

- Rule name, source, destination, trigger type, last result, and next run time.
- Actions: run now, preview, edit, duplicate, export, enable/disable, and delete.

### 10.2 Rule Editor

- Basic information: name, source, and destination.
- Files to copy: source tree/list selection with tri-state checkboxes and match preview.
- Copy options: comparison, symbolic links, concurrency, and retries.
- Trigger: manual, watch for changes, fixed interval, or cron.
- Validation before save, with dry run as the primary confirmation mechanism.

### 10.3 Jobs and Logs

- Current phase, file/byte progress, throughput, elapsed time, and cancel action.
- A virtualized file-operation list filterable by level, result, and path.
- Historical job details and log export.

## 11. CLI Conventions

| Exit code | Meaning |
|---:|---|
| 0 | Success, or a dry run without errors |
| 1 | Job completed with one or more file failures |
| 2 | Invalid arguments or rule |
| 3 | Endpoint unavailable or job could not start |
| 4 | Job cancelled |
| 10 | Unclassified internal error |

Human-readable logs go to stderr by default and the final summary goes to stdout. With `--json`, stdout contains only stable structured events for reliable script consumption.

## 12. Data and Compatibility Strategy

- Increment `schemaVersion` only for incompatible changes.
- Keep version-by-version migrators such as `v1 -> v2`; migrate old rules before validation.
- Reject unknown newer schema versions with a clear error instead of guessing.
- Internally stored rules contain management fields such as `createdAt` and `updatedAt`; exports contain executable configuration and required metadata only.
- Credentials never appear in rule JSON or logs. Future versions store opaque values such as `credentialRef`.

### 12.1 V1 File-based Persistence

V1 uses ordinary files in the platform application-data directory:

```text
app-data/
  settings.json
  rules/
    <rule-id>.json
  jobs/
    <job-id>/
      summary.json
      events.jsonl
```

- Each rule is stored independently so corruption or a failed write does not affect every rule.
- Settings and rule files are written to a temporary file and atomically replaced after a successful flush.
- Each job writes a compact `summary.json` and appends detailed structured events to `events.jsonl`.
- The recent-job list is built from summary files. A lightweight cache index may be added for startup performance, but it must remain disposable and fully rebuildable.
- Retention runs by age and total storage size. It removes complete old job directories rather than editing log files in place.
- The persistence layer exposes `RuleRepository`, `SettingsRepository`, and `JobRepository` interfaces. The copy engine never depends directly on the file layout.
- The desktop application and any background process must use an ownership lock to prevent concurrent rule writes and duplicate scheduler ownership. CLI execution of an exported rule does not require access to application storage.

SQLite is intentionally excluded from V1 to keep packaging, migrations, recovery, and multi-process locking simple. A database-backed repository can be added later without changing the domain model when actual requirements justify it, such as tens of thousands of jobs, complex cross-rule queries, reporting, or frequent concurrent writes.

## 13. Observability and Privacy

- Log levels are error, warn, info, debug, and trace. The default is info. Per-file success may be debug, while per-file failure must be error or warn.
- Each job receives a `jobId` shared by the UI, CLI, logs, and future remote requests.
- Logs are retained for 30 days by default or removed when a capacity limit is reached, whichever occurs first.
- Diagnostic exports replace root paths with endpoint aliases by default. Including relative paths requires explicit user confirmation.

## 14. Test Strategy

- **Unit tests:** rule migration/validation, path normalization, selection state machine, comparison policies, and schedule calculation.
- **Integration tests:** create, overwrite, selection, cancellation, retry, damaged temporary files, and permission errors in temporary directories.
- **Platform tests:** Windows long paths and file locks, macOS/Linux permissions and symbolic links, case sensitivity, and Unicode names.
- **Property tests:** relative paths can never escape an endpoint root; descendants of a deselected directory never enter a plan.
- **End-to-end tests:** create and export a rule in the UI, then dry-run and execute it in the CLI; verify identical results.
- **Fault injection:** disk full, destination disappearance, source changes during copy, lost watcher events, and process interruption.

## 15. Open Product Decisions

1. Does V1 require mirror deletion? Recommendation: no; add it only after trash/recovery, delete preview, and accidental-deletion safeguards are mature.
2. Must automatic and scheduled synchronization continue after the UI exits? If yes, V1 needs a background process, single-instance locking, and launch-at-startup design.
3. Is cron required in V1, or are fixed intervals sufficient?
4. Is a portable, installation-free build required? This affects the default application-data, rule, and log locations.
5. What is the launch-platform priority, and are code signing and automatic updates required?
