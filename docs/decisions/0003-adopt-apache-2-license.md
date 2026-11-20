# 0003: Adopt the Apache License 2.0

- Status: Accepted
- Date: 2026-11-17
- Supersedes: [ADR 0001](0001-retain-mit-license.md)

## Context

ADR 0001 retained the repository's existing MIT license while the project
standards were adopted, because changing legal terms required explicit owner
authorization. The project owner has now authorized using the Kaven standard
license.

## Decision

License Kaven File Copy under the Apache License 2.0 (`Apache-2.0`). Use the
canonical Kaven Apache license text at the repository root and declare the same
SPDX identifier in the Cargo workspace. User-visible license text must remain
consistent with the repository license.

Third-party components remain governed by their respective licenses.

## Consequences

Users receive the permissive rights and express patent-license grant defined by
Apache-2.0, subject to its notice and redistribution requirements. Future
source files, packages, release metadata, and documentation use Apache-2.0 as
the project license.

## Alternatives

Continuing with MIT was rejected after explicit authorization because it would
deviate from the shared Kaven baseline without a remaining product need.
