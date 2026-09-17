# MCP rich context layer

Status: landed 2026-09-17.

This document describes how the Knowlith MCP gateway serves **rich,
dependency-aware context** to agents in one call, without breaking the
approval gate or `check_coverage`.

Canonical product rules remain in [`ARCHITECTURE.md`](../ARCHITECTURE.md).

---

## Problem

Before this layer, the agent workflow was:

```text
get_relevant_context  →  titles only
get_context × N       →  one body at a time
lookup_value          →  table rows
check_coverage        →  audit
```

That is honest but slow and easy to get wrong: agents often read two of
eight listed titles and answer anyway. Competitors (RAGraph, Cortex hybrid
retrieval) return a rich pack in one retrieval pass. We needed the same
**richness** with our **provenance and approval** model.

---

## Solution: three layers

### 1. Graph metadata (`relations.why`, `relations.confidence`)

Model-proposed edges from the `relate` job now persist:

| Column | Meaning |
| --- | --- |
| `why` | One sentence explaining the dependency (from the compiler) |
| `confidence` | Trust for graph expansion: structural/manual = 1.0, model = 0.75 |

Edges below 0.7 confidence are skipped during context expansion unless
no alternative path exists.

The `relate` job re-runs when **`knowledge_fingerprint`** changes (hash of
live object ids + `updated_at` + title + body), not only when object count
changes.

### 2. Context packer (`crates/mcp/src/pack.rs`)

```text
question
  → hybrid discovery (FTS BM25 + title/body overlap + graph neighbour boost)
  → BFS foundations (depth 1–4, confidence filter)
  → topological pack (foundations before primaries)
  → optional table row hook when body mentions prices
  → record tool_reads for every served object
  → open case for check_coverage
```

Session cache (`pack::Snapshot`) holds objects + edges while
`servable_revision()` is unchanged — avoids reloading the lake on every tool
call in one conversation.

### 3. MCP tools

| Tool | Role |
| --- | --- |
| **`get_task_context`** | **Primary.** Full pack: bodies, all evidence quotes, inlined foundations, case id. |
| `get_relevant_context` | Lightweight title map; **does not** record reads. |
| `get_context` | One object; `includeFoundations: true` (default) inlines depth-1 prerequisites. |
| `check_coverage` | Unchanged — finish every task with this. |

### Health gate (`crates/mcp/src/health.rs`)

When the lake is empty or compile/settle jobs are queued, listing tools
return an honest message instead of “nothing found”.

---

## Agent workflow (system prompt)

The MCP `initialize.instructions` and client guidance files now say:

1. **`get_task_context`** for real work
2. **`lookup_value`** for every figure
3. **`check_coverage`** before claiming the company's rules were checked
4. Open questions stay open

---

## What is deliberately not built yet

| Item | Notes |
| --- | --- |
| Vector / sqlite-vec index | FTS + graph re-rank first; vectors remain a derived index per `ARCHITECTURE.md` |
| Entity resolution | Next P2 item (RAGraph pattern) |
| SMB `CHANGE_NOTIFY` | Windows NAS notify; local roots use `notify` (landed) |

---

## Files

| Area | Path |
| --- | --- |
| Packer | `crates/mcp/src/pack.rs` |
| Health | `crates/mcp/src/health.rs` |
| Tools | `crates/mcp/src/tools.rs` |
| Instructions | `crates/mcp/src/lib.rs`, `crates/desktop/src/guidance.rs` |
| Schema | `crates/lake/src/schema.sql`, `crates/lake/src/migrate.rs` |
| Relate trigger | `crates/worker/src/lib.rs`, `crates/lake/src/serve.rs` |
| Tests | `crates/mcp/tests/session.rs` |

---

## Success criteria

1. One `get_task_context` call returns primary rules **and** their foundations with quotes.
2. Every served object is recorded in `tool_reads` (unlike `get_relevant_context`).
3. `check_coverage` still reports what was offered but never read.
4. Model edges carry `why` in the lake; low-confidence edges do not expand context.
5. Editing an approved object re-triggers `relate` when the fingerprint moves.
