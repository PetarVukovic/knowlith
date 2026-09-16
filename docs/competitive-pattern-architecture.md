# Competitive pattern architecture for Knowlith

Status: P0 primitives landed in tree (2026-09-16). Event-watch and portable
Markdown export remain open.

This document maps external open-source projects onto Knowlith’s existing
shape, names what we already own, what is worth adopting, and what we must
not copy. It is pattern confirmation, not a competitor clone list.

Canonical system shape remains [`ARCHITECTURE.md`](../ARCHITECTURE.md).
This file only answers: *where do neighbouring projects strengthen our next
moves?*

---

## Target stack (Knowlith-shaped)

```text
LOCAL EDGE
Rust daemon (same process as today, unless a true remote share requires isolation)
│
├─ allowed roots          (sources the owner added)
├─ deny patterns          (secrets / keys — gap today)
├─ write-settle detection (file finished writing — gap today)
├─ checksum / content hash (sha256 + text_sha256 — already)
├─ SQLite local state     (lake — already)
├─ watch + periodic reconciliation
└─ OS / SMB credentials stay on the machine that can see the share
      ↓

RAW LAYER
immutable source refs + extracted text/tables + source spans
      ↓

COMPILER
facts · rules · process steps · conflicts · skill candidates
      ↓

INBOX / CANDIDATES
never direct-write canonical / approved
      ↓

HUMAN REVIEW
approve / edit / reject
      ↓

CANONICAL CONTEXT
approved objects (stable IDs) · optional portable Markdown snapshot
      ↓
derived indexes (rebuildable):
graph · (future) vectors · SQLite search
      ↓

MCP GATEWAY  (already)
get_relevant_context · search_context · get_context · lookup_value
get_process · get_skill · get_source_evidence · what_breaks_if
check_coverage · propose_change
```

**Naming collision.** Knowlith’s job kind `settle` means *consolidate
candidates into objects*. Several filesystem projects use “settled” for
*the file has finished being written*. Those are different primitives. This
document calls the filesystem one **write-settle**.

---

## What Knowlith already has (do not rebuild)

| Pattern | External confirmation | Knowlith today |
| --- | --- | --- |
| Inbox → curator → canonical | [cortex-brain](https://github.com/sani-savaliya/cortex-brain) | `candidates` → evidence gate → human approve → MCP |
| Content hash, not mtime | [Digital-Defiance/mcp-filesystem](https://github.com/Digital-Defiance/mcp-filesystem) | `sha256` + `text_sha256`; unchanged bytes skip compile |
| Raw → structured layers | [aws-samples/sample-knowledge-acquisition-skill](https://github.com/aws-samples/sample-knowledge-acquisition-skill) | extract → candidates → consolidate → relate/skills |
| Open questions | cortex-brain | gate + company card; agent must not invent answers |
| Skills as prompts + tools | [sulfierry/mcp](https://github.com/sulfierry/mcp), [kdpa-llc/local-skills-mcp](https://github.com/kdpa-llc/local-skills-mcp) | `prompts/*` + `get_skill` + `listChanged` |
| Lazy context (titles first) | local-skills-mcp | `get_relevant_context` returns titles; body via `get_context` |
| Coverage over a finite set | — (Knowlith-specific) | `check_coverage` |
| Derived graph | second-brain / gbrain style | `knowlith-graph` rebuildable from approved objects |

The product thesis stays: **agents never get a raw filesystem MCP**. The
edge reads folders; the gateway serves *approved* knowledge. Exposing
`list_files` / `read_file` on company shares to Claude would undo that.

---

## External references (by concern)

### Edge / filesystem / NAS

| Project | URL | Primitive to take |
| --- | --- | --- |
| **renfield-mcp-filesystem** | https://github.com/ebongard/renfield-mcp-filesystem | Watch local + SMB; **write-settle** via `CLOSE_WRITE` / SMB2 `CHANGE_NOTIFY` + debounce; push to backend over REST; **credentials and share access stay on the edge** — backend never mounts the share |
| **filesystem-mcp** (j0hanz) | https://github.com/j0hanz/filesystem-mcp | Strict **allowed roots**; sensitive-file denylist (`.env`, PEM/SSH keys); subscriptions / change notifications; batch reads; stdio + HTTP |
| **mcp-filesystem** (Digital-Defiance) | https://github.com/Digital-Defiance/mcp-filesystem | Directory watching + **checksums** + recursive sync + atomic file ops |
| **filesystem-mcp** (achetronic) | https://github.com/achetronic/filesystem-mcp | OAuth, JWT/RBAC, path policy via globs/CEL — **V2 signal only** (“Sales AI may read `/Sales/**`, not `/Finance/**`”) |

Related (not primary): [nithiin7/remote-file-server-mcp](https://github.com/nithiin7/remote-file-server-mcp) — SMB bridge with denylist and audit log; reinforces deny patterns + credentials-not-in-chat.

### Raw → structured knowledge

| Project | URL | Primitive to take |
| --- | --- | --- |
| **sample-knowledge-acquisition-skill** | https://github.com/aws-samples/sample-knowledge-acquisition-skill | Immutable `raw/`; derived entity/concept/comparison markdown; **`SCHEMA.md` taxonomy** before free-form extraction |
| **company-brain-builder** | https://github.com/adambaitch/company-brain-builder | Research → interview → generate structured markdown brain (input: website + questions; for us: documents + NAS + cloud) |

### Governance / canonical store

| Project | URL | Primitive to take |
| --- | --- | --- |
| **cortex-brain** | https://github.com/sani-savaliya/cortex-brain | Agents write only to `inbox/`; curator promotes; freshness lifecycle; open questions for conflicts |
| **second-brain** | https://github.com/stancsz/second-brain | Markdown (or exportable files) as durable truth; SQLite as **disposable search index** |
| **memstem** | https://github.com/Memstem/memstem | Same principle stated explicitly: markdown canonical + SQLite hybrid index |
| **gbrain** | https://github.com/garrytan/gbrain | `skills/<name>/SKILL.md`, trigger verbs, procedures; company-brain slice; `skillify`-style scaffolding as a **Process → Skill** compiler target |

### Skills / MCP surface

| Project | URL | Primitive to take |
| --- | --- | --- |
| **local-skills-mcp** | https://github.com/kdpa-llc/local-skills-mcp | Lazy load: name/description first; full `SKILL.md` only when needed |
| **Skills MCP** (sulfierry/mcp) | https://github.com/sulfierry/mcp | Skills as `list_skills` / `search_skills` / `get_skill` (+ outline mode) — skill is resource + helper tools, not one mega-tool |
| **graph-of-skills** | https://github.com/davidliuk/graph-of-skills | Offline dependency graph over `SKILL.md`; runtime returns a **small relevant subset + prerequisites** |
| **MCP ext-skills** (working group) | https://github.com/modelcontextprotocol/ext-skills | Direction of travel for skills over MCP primitives |

---

## Gap analysis (Knowlith vs patterns)

### Already strong — keep as-is

1. **Candidate → review → approved** matches cortex-brain; compiler must not overwrite production without a person.
2. **Mechanical evidence gate** is stricter than any of the referenced projects.
3. **Content hashing** already answers “bytes changed”, not only mtime.
4. **MCP gateway tool surface** already matches the target list above.
5. **`get_relevant_context` titles-first** is the local-skills lazy pattern for *context*, not only skills.
6. **NAS awareness at product level** (`large_scan`, network-share path field, battery policy) — incomplete as an edge, but intentional.

### Real gaps worth closing

| Priority | Gap | Steal from | Knowlith shape |
| --- | --- | --- | --- |
| **P0** | No **write-settle** before extract; rescans are periodic | renfield-mcp-filesystem | Debounce after create/write events; then hash; then queue `compile_document`. Keep hourly/periodic rescan as reconciliation net |
| **P0** | Weak **deny patterns** (`~$…`, `.DS_Store`, `Thumbs.db` only) | j0hanz filesystem-mcp | Skip `.env`, `*.pem`, `id_rsa`, keystores, etc.; report as owner-visible skip reasons |
| **P0** | Compiler lacks **company schema** (“what this company is”) | AWS knowledge-acquisition + company-brain-builder | Owner answers via UI → lake; worker reads them. Already named in `ARCHITECTURE.md` Open shape |
| **P1** | Mostly **poll**, not event watch | renfield + Digital-Defiance | fs events for local roots; SMB notify when available; rescan remains safety net |
| **P1** | SQLite is runtime source of truth; no **portable approved snapshot** | second-brain / memstem / OKF-adjacent | Keep lake for queue, evidence, tool_reads; add exportable Markdown/OKF of *approved* objects so indexes stay rebuildable from files |
| **P2** | Skill / rule subset is relevance-based, not **dependency-aware** | graph-of-skills | Use existing `knowlith-graph` edges to trim what `get_skill` / case packs pull |
| **P2** | True **edge process** for unmounted SMB | renfield | Only when daemon must not hold share credentials (separate box / isolation). Mounted `\\NAS\` on the owner machine does not require a second binary yet |
| **Later** | Path-level RBAC per agent | achetronic | Multi-agent / multi-role policy — not V1 |

### Explicitly do not adopt

| Anti-pattern | Why |
| --- | --- |
| Agent-facing raw filesystem tools | Violates “approved knowledge only”; undoes coverage and evidence story |
| Renfield **create-only / ignore rewrite** | Knowledge bases must re-compile when `sha256` moves |
| OAuth/JWT/CEL path policy in V1 | One owner, one lake; complexity without a second trust domain |
| Splitting edge “because Renfield did” while the share is already mounted | Extra process with no credential boundary |

---

## Proposed architecture moves (approval gate)

No code until these are accepted.

### 1. Write-settle + deny on the existing daemon

```text
fs event | periodic rescan
    → path under allowed root?
    → deny pattern?  → skip + reason
    → write-settle debounce (esp. SMB / copy-in-progress)
    → content hash
    → known hash? → noop
    → extract → compile_document …
```

References: renfield (events + settle), j0hanz (allow/deny), Digital-Defiance (checksums).

### 2. Company profile before stage 2

```text
UI questions → lake settings / company profile
    → structural stage can use taxonomy
    → candidates prompt names domains, not invoice schema loops
```

References: AWS `SCHEMA.md`, company-brain-builder interview loop.
Constraint (from `ARCHITECTURE.md`): worker has no stdin — answers travel through the interface and the lake.

### 3. Portable approved export (files as backup truth)

```text
approve transaction
    → lake remains operational source of truth
    → write/update Markdown (or OKF) snapshot for that object
    → graph / future vectors rebuild from approved set + snapshots
```

References: second-brain, memstem, cortex-brain markdown brain.
We do **not** invert day-one storage: queue, leases, evidence verification, and `tool_reads` stay in SQLite.

### 4. Company brain (live graph) + assistant ports

```text
approved objects + relations (lake)
    → GET /api/brain  (rebuild each call; no second store)
    → Company brain UI (nodes / edges)
    → Ask AI on a node:
         desktop surface → Claude Desktop / ChatGPT / Cursor.app (outside)
         terminal surface → Claude Code / Codex CLI / Cursor Agent (Terminal)
```

The graph is derived, never authored by hand. Simple-mode Home stays claim-
first; Company brain is a separate surface for walking approved knowledge and
opening a connected assistant against a node.

### 0. Test how well AI knows the company (Home)

```text
Home CTA → pick connected assistant by launch surface
    → deep link (desktop) or Terminal session (CLI)
    → prompt: get_relevant_context + check_coverage
    → History proves reads; the model’s claim alone does not
```

References: company-brain-builder (interview/brain shape), graph-of-skills,
existing try deep links + CLI spawn.

---

## Mapping: external stack → Knowlith crates

| Layer | Crate / surface | Change class |
| --- | --- | --- |
| Allowed roots | `lake` sources + `worker` walk | already |
| Deny patterns | `extract::is_noise` (or sibling) + worker/cli walk | small |
| Write-settle | `worker` (+ optional `notify` / SMB notify) | medium |
| Checksums | `core` / `extract` | already |
| Raw + spans | `extract` + documents table | already |
| Candidates / inbox | `compiler` + `lake` | already |
| Human review | `server` + interface | already |
| Canonical objects | `lake` objects (+ optional file export) | small–medium |
| Derived graph | `graph` | already; skill packing uses it later |
| MCP gateway | `mcp` | already; refine packing later |
| Company schema | `server` + `lake` settings + `compiler` prompts | medium — product |

---

## Success criteria (when we implement)

1. A half-written NAS file is not extracted; after write-settle + hash, it is.
2. Dropping `.env` or a PEM into a watched folder never enters the lake; the work panel names the skip class.
3. With a company profile filled in, compiling a folder of invoices does not produce twenty “Invoice number” objects as the main output.
4. Agents still cannot list or read arbitrary paths under a source root.
5. Every claim in the interface and MCP still cites a document row (Knowlith’s one rule).

---

## Decision log

- [x] Approve P0 trio: write-settle, deny patterns, company schema
- [ ] Choose portable export format (plain Markdown under `~/Knowlith/` vs OKF bundle)
- [ ] Defer true SMB edge process until a non-mounted / credential-isolated deployment exists
- [ ] Defer path RBAC until a second agent role exists
- [ ] Event-driven fs watch (local + SMB notify); periodic rescan remains the safety net

### Landed (2026-09-16)

- **Deny / secret patterns** in `knowlith-extract::is_secret` (+ walk skip counts).
- **Write-settle** (3s mtime debounce) in `knowlith-worker` rescan before extract.
- **Company profile** in Settings → lake `company_profile` → compiler stage-2 instructions.
- **Cursor** as a first-class AI tool (`~/.cursor/mcp.json` + rules guidance).

When the remaining items are decided, replace open checkboxes with a dated
implementation plan and link it from `tasks/todo.md`.
