# 0001: Retain the MIT license

- Status: Superseded by [ADR 0003](0003-adopt-apache-2-license.md)
- Date: 2026-11-17

## Context

Kaven engineering standards use Apache-2.0 as the default for new projects but
allow a different project license when the choice is explicit and documented.
Kaven File Copy already declares MIT in its Rust workspace and presents that
license in the desktop UI.

## Decision

Retain the MIT license and add the complete license text at the repository
root. Third-party dependencies remain governed by their own licenses.

## Consequences

The project does not receive Apache-2.0's express patent-license language. The
license remains short, permissive, and consistent with the existing manifests
and UI. Any future license change requires a separate legal and compatibility
review.

## Alternatives

Changing immediately to Apache-2.0 was rejected because a repository hygiene
change should not silently alter the project's legal terms.

