# Knowlith

Your company knowledge, compiled for every AI.

A Rust daemon that reads a company's folder and turns it into knowledge that
can be traced back to the sentence it came from, and a React interface that
shows it. They talk over HTTP on `127.0.0.1`. With the daemon stopped the
interface falls back to a demo company and says so in the status bar, so you
can see what the product does before installing anything.

## Testing it from nothing

```sh
sh scripts/dev.sh                                # daemon and interface together, hot reload
```

Then open `http://localhost:5173`, name the company, and point it at a
folder. The chooser is the machine's own — the daemon opens it, because a
browser is never told where a folder is.

Or drive the whole thing from the command line:

```sh
sh scripts/reset.sh --yes                        # back to before it was installed
sh scripts/full-test.sh                          # the test company, no provider, no cost
sh scripts/full-test.sh ~/Documents/YourCompany  # your folder, your own AI command line
sh scripts/verify-gateway.sh                     # what an AI tool actually sees
```

With no argument the compile replays recorded engine replies, so it costs
nothing and gives the same numbers every time. With a folder it uses
whichever AI command line is installed, on your own subscription, and says
so before it starts.

Neither approves anything. That is the product, so the run stops at the
review queue and hands over.

## Install

One command. Nothing is sent anywhere, nothing needs an administrator, and
everything it writes lives under your home directory.

```sh
# macOS and Linux
curl -fsSL https://raw.githubusercontent.com/OWNER/knowlith/main/install.sh | sh
```

```powershell
# Windows
irm https://raw.githubusercontent.com/OWNER/knowlith/main/install.ps1 | iex
```

Then:

```sh
knowlith scan ~/Documents/YourCompany   # read a folder
knowlith serve                          # the interface, on http://127.0.0.1:7717
knowlith connect                        # hand it to Claude and Codex
knowlith autostart on                   # keep working when the window is closed
```

The interface is compiled into the binary, so `serve` opens a real interface
on a machine that has never seen Node.js. Removing Knowlith is deleting
`~/Knowlith`, the binary, and whatever `knowlith autostart off` and
`knowlith disconnect` leave behind — which is nothing.

## The Rust side

```
crates/core/     domain types and the mechanical evidence check — no I/O, no model
crates/extract/  PDF · DOCX · XLSX · CSV · Markdown · text → blocks with byte offsets
crates/lake/     SQLite: the evidence gate on write, versions, the graph, the job queue
crates/graph/    impact, propagation order and cycle detection over approved objects
crates/engine/   where the model runs: the owner's own CLI as a child process, or a replay
crates/compiler/ documents to knowledge; reading, settling, relating, drafting skills
crates/server/   the local HTTP API, and the interface baked into the binary
crates/worker/   the loop that drains the queue, so work happens without being asked
crates/mcp/      the gateway: approved knowledge served to AI tools over stdio
crates/desktop/  what this machine is — paths, app configs, login service, the icon
crates/cli/      the `knowlith` command
```

Nothing in `core`, `extract` or `graph` reaches the network or calls a model,
and the compiler only does so through a swappable engine. That is what lets
the whole suite run on any machine without a provider key.

```sh
cargo test --workspace
```

### Try it on a folder

```sh
cargo run -p knowlith-cli -- scan "fixtures/termoval/source/Termoval - Prodaja"
cargo run -p knowlith-cli -- search "placanja"
cargo run -p knowlith-cli -- doctor
cargo run -p knowlith-cli -- engines
cargo run -p knowlith-cli -- ask "List every rule, verbatim." \
  --document "fixtures/termoval/source/Termoval - Prodaja/04 Servis/Vrijeme-izlaska-na-teren.txt" \
  --engine claude
```

### The whole loop, locally

Two ways round, and they produce the same lake.

```sh
# Hands off: read the folder, then let the worker do the rest.
cargo run -p knowlith-cli -- scan "fixtures/termoval/source/Termoval - Prodaja"
cargo run -p knowlith-cli -- work --once --replay fixtures/termoval/cassettes

# Or serve the interface and work in the background at the same time.
cargo run -p knowlith-cli -- serve --port 7717 --replay fixtures/termoval/cassettes
```

`--replay` uses the recorded replies and calls nothing; `--engine claude` or
`--engine codex` does it for real, and `--record <dir>` saves the replies so
the run can be repeated offline.

Step by step, if you want to watch each stage:

```sh
cargo run -p knowlith-cli -- compile --replay fixtures/termoval/cassettes   # read + settle
cargo run -p knowlith-cli -- relate  --replay fixtures/termoval/cassettes   # the graph
cargo run -p knowlith-cli -- merges                                         # what needs deciding
cargo run -p knowlith-cli -- skills  --replay fixtures/termoval/cassettes   # after approving a process
cargo run -p knowlith-cli -- serve --port 7717
```

On the checked-in fixtures that produces 51 objects from 13 documents, three
conflicts, 32 claims refused — mostly price rows, which are looked up rather
than approved — seven pairs to decide about, and 55 graph edges.
`knowlith doctor` re-checks every stored quote.

The lake defaults to `~/Knowlith/data/lake.sqlite`; pass `--db` for another
path. `scan --dry-run` counts without storing.

### Six decisions worth knowing before reading the code

**Offsets point into the rendition, not the file.** For a PDF or a
spreadsheet, "byte 4120 of the file" means nothing — the file is a container.
Each document therefore carries a deterministic text rendition and its hash,
and every offset is into that. For Markdown, text and CSV the rendition *is*
the file, and `verbatim` says so. When a parser changes, `text_sha256` moves
even though the file did not, which is exactly when stored evidence has to be
rechecked — `knowlith doctor` reports what no longer holds.

**Evidence cannot be stored unless it verifies.** There is no unchecked insert
path in `knowlith-lake`. A quote is compared against the bytes it claims to
come from, with whitespace normalised and nothing else forgiven. A plausible
sentence the document does not contain is refused, and takes the whole object
with it.

**The engine is a child process, and it inherits nothing.** `codex` and
`claude` are spawned per job, given one document on stdin, and exit. They run
read-only, in an empty temporary directory, with the owner's own settings,
hooks, MCP servers and project instruction files switched off — the first real
run of this code returned an unrelated hook's warning as part of the answer,
which is both noise and a hint that the compiler's output would otherwise
depend on whose laptop it ran on. Claude Code's `--bare` would isolate more
but never reads OAuth, so it would silently break "runs on your existing
subscription"; `--restricted --strict-mcp-config` in an empty directory gets
the isolation and keeps the login. A lost connection and a refusal are
different errors, and only the first is retried.

**Reading a document and deciding what it means are separate jobs.** Stages 1
and 2 read one document and cost one model call; stages 3 and 4 compare
everything against everything and cost nothing. Keeping them together looked
natural and was wrong in a way that only shows up in the background worker:
compiling documents one at a time asks "is this claim current", "does this
supersede that" and "do these disagree" with a sample size of one, and finds
no conflicts at all. So what the engine says is stored per document in
`candidates`, and the deterministic half re-runs over the whole set whenever
the set changes. A document whose text has not moved is never read again; a
thirteenth document makes the first twelve worth comparing again, not worth
re-reading.

**The graph is proposed by a model and marked as such.** Structural edge
detection — one object's title appearing in another's text — found **one edge
across fifty-one objects** on the real fixture folder, and a graph with one
edge answers "what breaks if I change the discount rule" with silence, which
reads as "nothing". Asked once about the whole set, an engine found
twenty-eight dependencies, including that the discount procedure rests on the
turnover threshold and on the rule forbidding more than 10% — neither of
which quotes the other. An edge has no sentence behind it, so the evidence
gate has nothing to check it against; it is stored with
`RelationOrigin::Model`, shown as *suggested* wherever it appears, and can be
removed. Edges are held to a lower bar than claims on purpose: a claim
asserts something about the company and a wrong one is a lie, while an edge
says "look at this too", and for that question over-warning is the safe
direction.

**The graph updates in the approval transaction.** Snapshotting the previous
version, publishing the new one, rewriting the edges and marking every
dependent as needing attention either all happen or none do. That is why there
is no second graph database: a separate store would have to be kept honest
with the objects it describes, and at a few hundred objects `petgraph` rebuilds
the whole thing in microseconds.

## The interface

```sh
sh scripts/dev.sh                 # both halves
cd knowlith && npm run dev        # the interface alone, against a daemon you started
```

React 19, Vite, Tailwind v4, Radix primitives. Two modes throughout: **Simple**
hides paths, object ids, byte offsets, raw JSON and numeric confidence;
**Engineer** shows them.

### Adding a folder

A browser cannot tell a page where a folder is — `webkitdirectory` gives
relative names and `showDirectoryPicker` gives a handle with no path — so the
page asks the daemon to open the system chooser, and the daemon answers with
a real path. There is a text field beside it for a network share the chooser
will not show.

Either way the folder is counted before anything is opened: files, size,
types, duplicates, names that look like last year's copy, and the types that
cannot be read, named rather than summed. Then `POST /api/sources` records it
and queues a walk — the interface never scans, so closing the window does not
stop the reading.

Onboarding runs at `/onboarding` and goes through the same two calls.

Every other screen reads from the daemon at `http://127.0.0.1:7717`
(`VITE_KNOWLITH_API` overrides it). The connection is checked once per load, so
the interface never shows half a real lake and half a demo; the status bar says
which it is.

## The test folder

`fixtures/termoval/` imitates a small Croatian HVAC company's shared drive,
including the mess that makes extraction hard:

- the same claim stated twice with different numbers (5% and 8% discount,
  24 h and 48 h response time, 15 and 30 day payment terms)
- last year's price list beside this year's
- a byte-for-byte copy under another name
- a PDF that is a scan, with no text layer
- a payroll spreadsheet that has to be held back
- Word lock files, `.DS_Store`, a CAD drawing, an empty document
- one note in Windows-1250 with CRLF line endings

Regenerate it with `fixtures/make_termoval.py` (needs `uv`). The golden
snapshot in `fixtures/termoval/expected/blocks.json` pins every block and
offset; update it deliberately with
`UPDATE_GOLDEN=1 cargo test -p knowlith-extract --test golden`.

## What the machine will not decide

Three questions have no answer this code can establish, and all three are
handed to the owner rather than guessed at. Each answer is kept, so the same
question is never asked twice.

**Is this the same rule written twice?** Subject overlap is measured on
diacritic-folded five-character stems — Croatian inflects almost every noun,
and `krugovi` and `krugova` are one concept and two strings — with
containment rather than Jaccard, because "Popust za stalne kupce" and "Popust
od 5% za stalne kupce" are the same rule with a qualifier. The measure is
tuned for recall, because it never acts: a false pair costs one dismissal, a
missed pair is a duplicate that stays forever.

**Do these two contradict each other?** Held to a stricter bar than a
duplicate, because the mistakes cost differently. A wrongly offered duplicate
costs a click; telling an owner their documents contradict each other when
they do not is the one claim this product must never make. So a contradiction
is only reported when neither title carries a content word the other lacks:
`Cijena montaže split sustava` and `Cijena montaže multi split sustava` name
two products at two prices and `multi` is exactly what separates them, while
`Popust za stalne kupce` and `Popust od 5% za stalne kupce` reduce to the same
words — and 5% against 8% for those customers is a contradiction nobody chose.
On the fixture folder this finds exactly one, which the four-stage compiler
misses entirely because the two claims land in different groups.

**Does this rule actually depend on that one?** Proposed by the engine, stored
as suggested, removable in one click. See the sixth decision above.

## Giving it to your AI tools

```sh
knowlith connect            # every application that is installed
knowlith connect codex      # or one of them
knowlith connect --dry-run  # show what would be written, write nothing
knowlith tools              # what each one currently says
```

Each application is edited in place: the entry is added, everything else in
the file stays, and a timestamped copy of the original is kept beside it. A
Codex `config.toml` keeps its comments, because it is edited as TOML rather
than as text.

Claude Desktop can also take it as an extension, which is the path that
shows the owner what it does before they enable it:

```sh
knowlith bundle --install    # writes ~/Knowlith/knowlith.mcpb and opens it
```

### What an agent can ask

Eleven tools, ten of which only read. Every answer carries the document, the
locator and the quote, so the agent can cite rather than assert.

| Tool | Answers |
| --- | --- |
| `get_relevant_context` | everything the company has decided that touches this piece of work |
| `search_context` | approved rules, processes and terms by their own words |
| `get_context` | one of them in full, with what it rests on |
| `lookup_value` | a figure, read out of the row it is written in |
| `get_source_evidence` | the passage in the owner's own document, word for word |
| `get_process` | how the company does something, in its own order |
| `get_skill` | a procedure the owner approved for an agent to run |
| `what_breaks_if` | what rests on a rule — the question a folder cannot answer |
| `list_pending` | subjects the owner has not decided, named but never answered |
| `check_coverage` | what the agent never looked at |
| `propose_change` | the one write: a suggestion, refused unless it quotes a real document |

`get_relevant_context` and `check_coverage` are the pair that matter. A
retrieval system cannot tell you what it did not return; here the approved
set is finite and related, so the things a question touches can be listed up
front and ticked off as they are read. An agent that has not read the
warranty is told so before it claims to have checked the company's rules.

The gate is the other half. An `approved` object is served in full; a
`conflicted` or `proposed` one has its **subject** named and its answer
withheld; a `superseded` or `rejected` one is not mentioned. Hiding an
undecided question does not stop an agent answering it — it removes the one
signal that would have stopped it.

## Seeing that a tool actually used it

Being in a configuration file proves somebody pressed a button. It does not
prove the company's knowledge reached a conversation, and those are the two
different things the **AI tools** screen now reports separately:

```text
Claude Desktop   Connected
                 Read 3 things · last 8 min ago

Codex            Connected
                 Has not read anything yet
```

Every serve is recorded against the application that asked, taken from the
`clientInfo` the client sends in `initialize`. A client we do not recognise
is reported as "another tool" rather than filed under one of these three.

**Activity** is the trail, one row per question rather than one per protocol
call:

```text
Claude Desktop                                       3 min ago
Working on: ponuda za stalnog kupca
  Read · 1
    Avans za radove iznad 5.000,00 EUR        rule
  Offered and not opened · 22
    Prag prometa kupca                        rule
    ...
  ⚠ Answered without checking what it had missed.
```

The second list is the part no retrieval system can produce. The approved
set is finite and the gateway said out loud what touched the question before
the agent chose, so what it skipped can be named.

Two things this deliberately does not claim. It reports what was **read**,
never why an answer came out the way it did — a model can answer from its
own context without calling a tool. And approving something does not report
"Claude updated": MCP has no acknowledgement, so the honest statement is
that it is effective for every conversation started from now.

## Working in the background

The daemon is registered with the operating system's own login service —
`launchd`, Task Scheduler, `systemd --user` — so closing the window changes
nothing. The queue is a table, so a crash, a closed laptop or a network that
comes back an hour later all resume rather than restart.

What it may do on its own is the owner's:

```
Automatically read changes   ·  Ask first  ·  Manual only
Pause model calls on battery
Ask before more than N files at once
```

Reading a folder, comparing documents and re-checking quotes are arithmetic
and always run. The three jobs that call a model are the ones these govern —
and a job that is held is **held**, not deferred, so a laptop left unplugged
overnight does not age its own queue into failure.

## The demo company

There is one, and it has to be asked for: `?demo` in the address, or
`VITE_KNOWLITH_DEMO=1`. Whenever the daemon answers, every screen shows
that lake and nothing else — an endpoint that fails renders empty rather
than falling back to a fixture, because another company's discount policy
appearing on your screen is indistinguishable from the product working.

With no daemon and no demo the screens are empty and the status bar says
Knowlith is not running.

## Known limits

**A price written in two formats used to be two figures.** `5.000`,
`5.000,00` and `5000` are now one amount, and a dot is only a thousands
separator when exactly three digits follow it, so `7.1` stays seven point one.
Amounts written in words are not handled.

**Two documents covering one subject with no figures are shown, not judged.**
The review screen lists both sentences and says the most recent wording is the
one in use. It does not assert that they disagree, because nothing here can
establish that.

**Skills rest on approved processes only.** A company with no approved process
gets no skills, which is the right order — a procedure an agent executes
should not rest on a rule nobody signed off — but it does mean the count is
zero until the owner approves something.

## Not implemented here

Knowlith Managed. `ManagedEngine` says so rather than pretending.

Windows is built and tested on every release, and every path, process and
configuration location goes through `crates/desktop` — but it has not been
run by hand on a Windows machine. The parts most likely to need a second
pass are the scheduled task and Claude Desktop's install location.

Per-application read counts. The gateway records every serve, but the
protocol does not carry which client asked, so the interface reports how
much has been served rather than by whom.

Everything else in this README runs.
