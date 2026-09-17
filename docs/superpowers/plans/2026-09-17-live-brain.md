# Live Company Brain Implementation Plan

**Goal:** Reliable incremental CLI builds with a truthful live 3D onboarding experience.

**Architecture:** Preserve the lake and worker; bound and checkpoint synthesis, derive a separate build graph, and share the renderer between build and approved views.

**Tech Stack:** Rust, SQLite, React, Three.js.

**Spec:** `docs/superpowers/specs/2026-09-17-live-brain-design.md`

## Global constraints

- English code and copy; preserve evidence and approval gates.
- No live paid model calls, index rebuilds, or unrelated edits.
- Use CodeGraph for navigation, verify stale results against source.
- Commit each verified stage; leave concurrent engine/test and lockfile changes alone.

## Execution

- [x] Supervisor: regress first-call corpus delivery and long-document coverage; replace external per-document chunk processes with bounded UTF-8 batches; persist successful outputs by content key; validate evidence and protect approved objects. Run `cargo test -p knowlith-supervisor` and commit.
- [x] Build projection: add HTTP tests that request `/api/brain/build`, assert proposed nodes retain status, rejected nodes are absent and `/api/brain` stays approved-only; implement the separate projection without assistant probes. Run `cargo test -p knowlith-server --test brain` and commit.
- [x] Renderer: test that adding a node preserves existing coordinates; make layout linear in nodes plus edges; retain radii and dispose resources; reuse graph data, honor motion preference and hidden visibility. Run frontend tests/build/lint.
- [x] Onboarding: replace inferred stages with queue-backed status and live graph; show real counts, failures and connection recovery; require explicit review continuation; prevent duplicate starts and await company persistence. Verify browser fixture states and commit together with renderer.
- [x] Runtime integration: inspect concurrent changes before touching engine; verify bounded process/output handling, supervise heartbeat and retry semantics; add missing regression coverage and commit only owned changes.
- [x] Final checks: run workspace tests and frontend checks, inspect committed diffs, document remaining limits and commit documentation.

## Verified outcomes

- Supervisor: first request contains source text; bounded UTF-8 batches cover long documents; successful requests are reused by content key; invalid quiz quotes are omitted and approved objects cannot be overwritten.
- Runtime: 8 MiB stdout/stderr limit, process-tree cleanup, deadline covers inherited pipes, temporary failures remain retryable, and supervision renews the worker lease.
- UI: shared 3D graph with no brain-shaped mesh (per owner correction), stable node positions, live saved discoveries, truthful build states, searchable list, business context and duplicate-start protection.
- Browser: production preview checked at 1365x1024 and 390x844 using fixture API responses; selection, fit, pauses, errors, reconnect and review readiness exercised. No JavaScript errors in the production preview. No paid model calls were made.
- Release: POSIX installer validates a binary before replacement; missing checksums fail closed; CI runs frontend, Rust and installer checks; release stays draft until all packages are uploaded.

## Practical limits

The build screen polls the durable lake projection; it does not stream model tokens. CLI billing follows the customer's own CLI configuration and provider limits. Full cloud execution is not part of this release. Large-graph layout is tested at 5,000 nodes; interactive GPU performance at that size is not benchmarked.
