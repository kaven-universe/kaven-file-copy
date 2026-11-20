# 0002: Temporarily accept transitive RustSec informational warnings

- Status: Accepted temporarily
- Date: 2026-11-17
- Review no later than: 2027-02-17

## Context

`cargo audit` reports no vulnerabilities after upgrading Tokio from 1.53.1 to
the patched 1.53.3. It still reports informational warnings inherited through
the current Tauri 2.11 dependency graph:

- `RUSTSEC-2026-0543` marks `chrono 0.4.45` unmaintained. Chrono is used by the
  public scheduling and persistence contracts, so replacing it requires a
  coordinated API and stored-data compatibility change rather than a patch
  dependency upgrade.

- `RUSTSEC-2024-0429` affects `glib 0.18.5`, which enters through Tauri's
  Linux GTK/WebKit stack. The affected `VariantStrIter` API is not used by this
  application's code, and Linux is not currently a release-validated target.
- `RUSTSEC-2024-0370` marks `proc-macro-error 1.0.4` unmaintained; it is also
  part of the Linux GTK macro dependency graph.
- `RUSTSEC-2025-0075`, `RUSTSEC-2025-0080`, `RUSTSEC-2025-0081`,
  `RUSTSEC-2025-0098`, and `RUSTSEC-2025-0100` mark the `unic` 0.9 crates
  unmaintained. They enter through Tauri's `urlpattern` dependency.
- `RUSTSEC-2026-0370`, `RUSTSEC-2026-0371`, and `RUSTSEC-2026-0372` mark
  `zvariant`, `zbus_names`, and `zvariant_derive` unmaintained. They are
  Linux-only transitive dependencies of `tauri-plugin-single-instance`; the
  application needs that plugin to enforce a single configuration writer.

The framework and single-instance warnings are transitive dependencies and
cannot be replaced locally without forking those upstream stacks. Chrono is
direct, but its replacement requires a planned compatibility migration.

## Decision

Accept these informational warnings temporarily while keeping `cargo audit`
in the release procedure. Do not enable Linux release distribution until the
`glib` warning is removed by an upstream Tauri/GTK update or a focused review
shows the shipped call graph cannot reach the affected iterator methods.

## Consequences

Windows release builds do not contain the GTK dependency chain. The
unmaintained Unicode crates remain in the locked cross-platform dependency
graph, so upstream Tauri releases must be reviewed regularly. Any RustSec
vulnerability (rather than an informational warning) still blocks release.

## Review condition

Review this decision on the stated date, before enabling Linux releases, or
whenever Chrono, Tauri, the single-instance plugin, `zbus`, `urlpattern`, GTK,
or WebKit dependencies are upgraded. Remove
the acceptance as soon as an upstream-compatible dependency set eliminates
the warnings.

## Alternatives

Forking Tauri/GTK and replacing `urlpattern` were rejected because their
maintenance and supply-chain cost is disproportionate for this pre-release
application. Ignoring all audit warnings globally was rejected because it
would hide new findings.
