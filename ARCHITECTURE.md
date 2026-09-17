# Architecture

Knowlith reads a company's own folders and compiles them into approved,
evidence-backed knowledge, which it then serves to that company's AI tools.
Nothing reaches a tool until a person has approved it, and nothing can be
approved without the sentence it came from.

This document is the shape of the system and the reasoning behind it.
`README.md` is how to run it; `CLAUDE.md` is how to work on it.

## The whole path

```
a folder on disk
      │
      │  extract — deterministic. Bytes in, blocks with offsets out.
      ▼
   documents ──────────────────────────────┐
      │                                     │  content hash decides what is new
      │  compile — four stages, one model   │
      ▼                                     │
   candidates ── evidence gate ── objects ──┘
      │              ▲
      │              └─ mechanical: the quote must be at those bytes
      │
      │  a person approves
      ▼
  approved objects ──► MCP gateway ──► Claude, Codex, another tool
      │                     │
      │                     └─ every read recorded, with which app asked
      ▼
   the interface: what is known, where it came from, who approved it,
                  and where it is being used
```

## The crates

Eleven, in a Cargo workspace. Edition 2024, resolver 3.

| Crate | Lines | What it owns |
| --- | --- | --- |
| `core` | 625 | Domain types and the mechanical evidence check. No I/O, no inference. |
| `extract` | 1369 | Files in, blocks with byte offsets out. XLSX, CSV, DOCX, PDF, text. No inference. |
| `lake` | 2606 | SQLite storage, the evidence gate on write, the durable queue, the policy. |
| `compiler` | 2955 | Four stages from documents to knowledge. Exactly one asks a model. |
| `engine` | 1230 | Where the model runs: the owner's own CLI as a child process, or a recorded replay. |
| `graph` | 308 | Impact, propagation order and cycle detection over approved objects. |
| `worker` | 823 | The background loop that drains the queue. |
| `mcp` | 2716 | The gateway. Approved knowledge served to AI tools over stdio. |
| `server` | 2410 | The local HTTP API, bound to `127.0.0.1`, behind a token. |
| `desktop` | 2712 | Where files live per operating system, and handing Knowlith to installed AI apps. |
| `cli` | 1273 | The `knowlith` command. |

The interface is React and Vite under `knowlith/`, compiled into the binary at
build time by `crates/server/build.rs`.

## Decisions worth understanding before changing anything

### Four compiler stages, not ten

Every stage that involves a model multiplies its own error into everything
after it. At 95% per stage, ten stages land near 60% — which is worse than
useless, because it is wrong in a way that looks right.

1. **Structural** — pick the blocks worth asking about. Deterministic.
2. **Candidates** — a model reads one document and proposes claims, each with
   the sentence it came from. *The only stage with an engine in it.*
3. **Consolidate** — group, rank by time and authority, mark conflicts.
4. **Relate / skills** — edges between objects, and skills from approved
   processes.

Where a format allows it, stage 2 barely runs: an XLSX price list is read
almost entirely deterministically, because a spreadsheet already has the
structure a model would otherwise have to guess.

### The evidence gate is mechanical before it is semantic

A model proposes a claim and names the span it came from. Before anything is
stored, the daemon reads those exact bytes out of the document and compares
strings. A quote that is not there is refused, whatever the model said about
it.

"A second model thinks this looks right" is not a gate. This one is arithmetic.

### Offsets point into the rendition, not the file

A PDF or a spreadsheet has no byte offsets a person could use. Extraction
produces a rendered text with blocks, and offsets address *that*. The rendition
is stored alongside the document so a quote can be re-checked years later
against the same text it was taken from.

### Two processes, one lake

`serve` (the daemon and its HTTP API) and `mcp` (the gateway a tool spawns) are
**separate processes** sharing one SQLite file in WAL mode. The daemon cannot
observe a live MCP session directly.

Everything the interface knows about tool usage therefore travels through the
lake. This is the constraint that shaped the whole of proof-of-use below.

### The queue is durable and the lease is not a flag

Work outlives the process that started it. A worker claims a job until a
timestamp, and the claim carries a **generation**. If the process dies, the
lease expires and the job returns to the queue with nobody having to notice
the crash. A heartbeat that still presents the previous generation is ignored,
and that holder kills its CLI child — otherwise two AI workers both extend
the same row and two agents answer the same prompt. Idempotency keys make a
retry recognise itself. Transport failure defers with a growing gap; a
refusal fails and is reported, because retrying it changes nothing.

Job kinds: `rescan`, `compile_document`, `settle`, `supervise`, `relate`,
`draft_skills`, `recheck`. Compile jobs may be claimed in batches by AI
workers (several documents, one CLI process). Settle still waits until the
compile queue — including leased jobs — is empty. With `relateAfterBuild`
(the default), `relate` waits until the owner has confirmed the build quiz
so the supervisor is not blocked behind a whole-folder model pass.

The daemon runs **one I/O track** (rescan / recheck) and **N AI tracks**
(default 2, policy-capped at 4) so a long folder walk does not starve
compile, and large folders amortise CLI cold-start.

### The interface queues; it never scans

Adding a folder records a source and enqueues a `rescan`. The walking is the
worker's. This is what makes "you can close this window" true rather than
reassuring.

## Proof of use

A configuration entry proves a button was pressed. It does not prove that
knowledge reached a conversation.

**Reads are attributed.** `clientInfo.name` arrives on every `initialize` and
is normalised in exactly one place, `App::from_client_name`, then carried on
every `tool_reads` row and every case. An unrecognised client stays
unattributed — "Another tool" — and is never folded into one of the three.
Telling an owner that Codex read their rules when it was Cursor is worse than
silence.

**Titles are not reads.** `get_relevant_context` returns titles and opens a
case, and deliberately records nothing. Recording it would inflate the counts
and destroy the coverage figure.

**Rich packs are reads.** `get_task_context` returns full approved text,
inlined foundations, and every evidence quote in one answer, and records a
`tool_reads` row for each object it serves. See [`docs/mcp-rich-context.md`](docs/mcp-rich-context.md).

**Coverage is possible because the set is finite.** The approved set is known
and the gateway named what it offered, so the interface can report what an
agent *skipped* — which nothing built on open-ended retrieval can do.

## The API is behind a token

The daemon listens on `127.0.0.1`, and for a while a comment in the router
claimed that made it private. It does not. Loopback keeps the *network* out; it
does nothing about the owner's own browser, which will carry a request to
`127.0.0.1` on behalf of any page they have open.

With `allow_origin(Any)` a website could:

- `GET /api/sources/preview?path=…` — map any folder on the disk;
- `POST /api/sources {"path":"/…"}` — have the daemon read one, then collect
  the text back through `/api/objects`;
- `POST /api/sources/browse` — open a native folder chooser on the desktop
  (no body, so no preflight to fail).

So every `/api` request must carry a secret written to `~/Knowlith/api.token`
at mode 0600. A page cannot read that file, and a header it cannot set is a
header no cross-origin request will carry. There is **no cross-origin allowance
at all**: in development Vite forwards `/api` to the daemon and attaches the
token in Node, so the browser is same-origin exactly as it is in the shipped
binary, and the token never enters a browser.

`route_layer`, not `layer`: a mistyped path must come back 404 from the
interface, not 401 from the guard. Reported as unauthorised, it sends whoever
is debugging it hunting a permission problem that does not exist.

The token defends against a page on *another* origin. It does not defend
against a page that becomes this one: a site at `evil.example:7717` whose DNS
answer is switched to `127.0.0.1` after the page has loaded is same-origin
with the daemon as far as the browser can tell, and may read `/` — and the
token written into it. The only thing that request still carries is the
domain the page was loaded from, in `Host`. So a second check, on the whole
router this time (the page included), refuses any `Host` that is not
`127.0.0.1`, `localhost` or `::1`. Found by asking the running daemon for `/`
with `Host: evil.example.com` and getting the token back.

## Showing the work

Every worker hands back a sentence when it finishes a job — `Cjenik 2026.xlsx:
14 claims · 2 not read`. Those sentences are kept on the job and served by
`GET /api/work`.

The **stage** (`reading`, `thinking`, `preparing`, `held`, `idle`) is derived
from what is still queued, never stored, so it cannot disagree with the queue.
When every outstanding job is held, the stage is `held` — a spinner over
"Reading the documents" while the banner beneath says twenty-four jobs are
waiting is the screen contradicting itself.

**Progress counts the current burst** — jobs queued since nothing was
outstanding — so a folder added today starts the bar at nought rather than at
last week's history.

A job that finished with nothing to report says nothing. The hourly `recheck`
would otherwise be the only line on an idle machine's panel, renewed every
hour.

## The company brain is a view, not a store

`GET /api/brain` is rebuilt from the lake on every call. It has no table of
its own, so it cannot drift from what the owner approved.

What it contains is exactly what answers questions, and nothing wider:

- **Approved objects only.** A candidate is not part of what the company knows
  yet, so it is not drawn.
- **Edges between two approved objects**, each carrying the sentence the owner
  reads on the arrow — `needs`, `used by`, `comes from`, `disagrees with`. The
  label is chosen once, on the server, so the map and the object page cannot
  disagree about what an arrow means.
- **Documents only where something approved quotes them**, joined by a
  `quoted in` edge per object. A folder of two hundred files that produced
  three rules shows three documents. Drawing the rest would show the owner a
  brain larger than the one that answers.
- The connected assistants, so a node can be opened in the tool the owner
  already uses.

The interface renders it with `3d-force-graph` on plain three.js — no React
wrapper, because the wrappers lag React majors and the graph is a mutable
scene, not a tree of components. The component mounts once and mutates: node
objects are reused by id across polls so positions survive a refresh, and a
highlight changes a material rather than rebuilding a mesh. Dragging nodes is
off; the library's drag controls fire a synthetic `pointerup` without a
pointer id that three's orbit controls cannot handle, and the owner's
interactions are click and hover.

**Asking happens outside Knowlith.** Clicking a node and choosing an assistant
calls `POST /api/tools/{app}/try`, which opens Claude Desktop, Codex or a
Terminal session on the owner's machine with the question prefilled. The MCP
plugin the owner connected during setup reads the lake; Knowlith does not run
a chat of its own beside the map.

## What updates by itself, and what the owner has to do

Three different answers, because three different things can change.

**Approved knowledge — automatic, nothing to do.** The gateway reads the lake
on every call. Approve a rule and the next question a running Claude Desktop
asks gets the new wording. There is nothing cached and nothing to reinstall.

**Skills and the prompt list — automatic for a running client.** A watcher
thread in the gateway polls `servable_revision()` (the count of approved
objects and the latest `updated_at`) every four seconds. When it moves it
sends `notifications/tools/list_changed`, `prompts/list_changed` and
`resources/list_changed`, and a client that is listening re-fetches. A skill
approved now becomes a prompt within seconds.

This is best-effort by design, and the product must not claim otherwise. MCP
has no acknowledgement: the notification can be written and never confirmed
applied, and a client that is not running at that moment gets nothing at all —
it simply asks afresh when it next starts. So the honest claim is never
"Claude ✓ updated" but "effective for every conversation started from now".

The manifest carries `prompts_generated: true` for this reason. Without it the
host takes the manifest's prompt list as the whole of it, and a skill approved
after the extension was installed would never reach the owner. `tools_generated`
stays `false`, because the tool surface really is fixed.

**The extension itself — manual.** `~/Knowlith/knowlith.mcpb` contains the
binary and a manifest. Upgrading Knowlith, renaming the company, or moving the
lake means rebuilding it (`POST /api/bundle`) and installing it again. Nothing
about a packaged extension updates in place.

## Storage: five primitives, not five copies

Each answers a different question, and only the first two exist today.

| Primitive | The question it answers |
| --- | --- |
| SQLite (`lake`) | What is stored, what is queued, what was read. The source of truth. |
| Canonical objects | What does this company say, in a form a person can read and edit. |
| Graph | What depends on this, and what breaks if it changes. Derived. |
| Vector index | What is this about, when nobody knows the right word. *Not built.* |
| CodeGraph | Where does the code disagree with the policy. *Not built.* |

The graph is **derived**. Canonical objects remain the source of truth, and the
graph can be rebuilt from them at any time.

## Where the model runs

Three placements, with different consequences for who pays and what may be
claimed about privacy:

1. **The owner's own CLI** as a child process — `claude` or `codex`, detected
   on `PATH`. The owner's own CLI carries the document text to that vendor,
   as it does for everything else the owner asks it; Knowlith adds no second
   recipient, and the owner's existing subscription pays. Stage 2 sends the
   whole rendition (eight documents per call in a batch), not a quote — the
   product must never say "only short quotes leave". Claude Code children
   are serialised (the CLI races concurrent processes); Codex and Cursor are
   not. The child is killed on Drop, timeout, or a lost lease. What it
   already said is a file under `data/runs/{job}/`, not state inside the
   process — a killed child must leave a session the next holder can read.
2. **A recorded replay** — `--replay <dir>` uses saved replies and calls
   nothing. This is how the test suite exercises the whole loop offline, and
   how `--record` produces new fixtures.
3. **Managed** — not available in this build.

`knowlith engines` reports what it can actually see.

## Open shape

The biggest known gap is not a bug. The compiler reads documents with no
context about **what the company is** — so a folder of twenty invoices yields
`Invoice number`, `Invoice parties`, `Invoice total and tax`: the schema of an
invoice, extracted twenty times. Correct, and useless.

Fixing it means asking the owner about their company before compiling, and the
constraint to design around is that **the worker has no user**: it runs in the
background, possibly with the window closed, so a child process cannot stop and
wait on stdin. The questions have to travel through the interface and the
answers into the lake, not through the engine's standard input.

## Incremental builds and live graph (v0.1.4)

The supervisor uses bounded UTF-8 source batches instead of truncating a whole-company prompt. Completed replies are checkpointed by request content and engine, so a retry can reuse completed work. The corpus is passed in the actual first request. Each request runs while the worker renews its lease; temporary engine failures stay retryable. Existing owner decisions are preserved and quiz evidence is checked against the source.

`GET /api/brain/build` is an authenticated owner projection of extracted documents and proposed/approved discoveries. It performs no assistant-process detection. It does not widen `/api/brain` or MCP visibility. Onboarding polls this projection and the durable work feed; it shows actual counts, pauses and errors, and waits for an explicit review action. Proposed discoveries are not presented as approved company knowledge.

Both graph screens share a clean 3D node-and-edge renderer, with no brain-shaped mesh. Identity-based coordinates survive new discoveries, document adjacency is built in linear time, unchanged snapshots do not reset the scene, and hidden tabs pause rendering. Reduced-motion preferences disable directional particles and camera transitions. A searchable list remains usable without WebGL.

The installer verifies checksums and checks that the downloaded executable runs before replacing an existing installation. A running daemon must be restarted to load a newly installed version; the installer explains this instead of silently claiming that the running UI was updated.
