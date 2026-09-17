# Working in this repository

Read `ARCHITECTURE.md` before changing anything that crosses a crate boundary.
`README.md` is written for whoever runs the product; this file is for whoever
edits it.

## The one rule everything else serves

**Knowlith may not tell the owner something the data underneath it does not
say.** A rule with no quote behind it, a tool credited with a read it never
made, a green tick over a company that has not been read, a spinner over a
queue that is paused — each of those is the same defect, and each has shipped
here at least once. When a screen or an API is about to make a claim, find the
row that backs it. If there is no row, the honest answer is the smaller one.

Three claims the product deliberately refuses to make, because they cannot be
checked:

- **"Claude ✓ updated".** MCP has no acknowledgement. `notifications/tools/
  list_changed` can be sent and never confirmed, and a client that is not
  running gets nothing. The truthful form is "effective for every conversation
  started from now".
- **"Why the assistant answered that".** A model can answer out of its own
  context without calling a tool. The Activity screen says what was *read*,
  never why an answer came out the way it did.
- **An estimated time for a compile.** A document takes a second or a minute
  depending on its size and the engine, and the first run has no history to
  predict from.

## Language

Everything that lands in the repository is in English: code, identifiers,
comments, docstrings, commit messages, documentation. Croatian appears only as
test fixtures and sample company data, because the product's first users are
Croatian companies and their documents are the real input.

## Before you write code

Walk the architecture first and get it approved. What happens, how the pieces
fit, what the final shape is. This applies to anything past a one-line fix.

## Conventions that are not obvious from the code

**Comments say why, not what.** Nearly every non-obvious block here carries the
reason it exists, usually the bug that produced it. Keep that. A comment that
restates the line above it is noise; a comment naming the failure the line
prevents is the only record of it.

**Tests are named after the claim, not the mechanism.** `a_website_cannot_make_
the_daemon_read_a_folder`, not `test_auth_middleware`. The mechanism is allowed
to change; the claim is not.

**Migrations are guarded on the table *and* the column.** `CREATE TABLE IF NOT
EXISTS` never adds a column, so every change to an existing table goes through
`crates/lake/src/migrate.rs`. Guard on both: a lake restored from a partial
backup may be missing the whole table, and an `ALTER TABLE` that fails takes
the open with it — before any screen can explain why. This has bitten twice
(`cases`, then `jobs`).

**Ids in the interface are a bug.** `From doc:9982ba86ea32837c` describes the
owner's own file in a vocabulary they cannot read. Resolve to the name on the
server, where the documents table is.

**A React key must identify the thing.** An id built from part of a span
(`{document}-{start}`) collides, and React silently drops a row — so the owner
sees less evidence than the object rests on. Identity means the whole thing.

**The frontend never pluralises by guessing.** `1 file walked`, not `1 files
walked`. Owner-facing sentences are generated in Rust; check them there.

## Running it

```sh
sh scripts/dev.sh              # daemon on 7717 + Vite on 5173, hot reload
sh scripts/dev.sh --fresh      # the same, starting from an empty company
```

`--fresh` moves the existing `~/Knowlith` aside rather than deleting it. A lake
is the only copy of what somebody approved.

Ctrl-C stops both. A Rust change needs a restart; a React change does not.

```sh
cargo test --workspace         # 370 tests
cargo clippy --workspace       # 5 pre-existing warnings in lake and compiler
cd knowlith && npx tsc --noEmit -p tsconfig.app.json && npx oxlint src
```

`cargo fmt` is **not** clean across this repository and running it would rewrite
almost every file. Match the surrounding style instead.

## Talking to the API by hand

Every `/api` route requires the daemon's token:

```sh
curl -H "x-knowlith-token: $(cat ~/Knowlith/api.token)" http://127.0.0.1:7717/api/health
```

Without it you get 401 and a sentence saying where the token is. This is not
optional plumbing — see **The API is behind a token** in `ARCHITECTURE.md`.

## Things that will waste your time if you do not know them

**The battery policy holds AI work.** `pauseOnBattery` defaults to true, so on
an unplugged laptop every `compile_document` job sits in `held` and nothing
appears to happen. The work panel says so; the command line does not.

```sh
curl -H "x-knowlith-token: $(cat ~/Knowlith/api.token)" -X PUT \
  -H 'Content-Type: application/json' \
  -d '{"processing":"automatic","pauseOnBattery":false,"largeScan":500}' \
  http://127.0.0.1:7717/api/policy
```

**`work --once` never enqueues `settle`,** so objects stay at zero and it looks
like the compiler produced nothing. Use `serve --replay fixtures/termoval/
cassettes` to exercise the whole loop offline.

**`--replay` finishes a small company between two polls.** Anything that waits
for the queue to drain will miss it. Watch what came *out* — document and
object counts — not the queue.

**Temp lakes in tests need a counter, not only a timestamp.** Two tests read the
same nanosecond, shared a file, and the second one's job was swallowed by the
first's idempotency key. It failed about one run in ten and looked like a queue
bug.

**`sqlite3` on the command line returns nothing under this session's tooling.**
Use `python3 -c "import sqlite3; ..."`.

## Index and memory tooling

Never create, rebuild or drop a zvec-grep index without being asked. claude-mem
captures sessions through hooks; there is nothing to call by hand.

## Not yet real

Honest about the prototype edges, so nobody builds on them believing otherwise:

- `StepAccess` is a checkbox. It requests no operating-system permission.
- Google Drive and OneDrive say "Coming soon" and are not started.
- `install.sh` / `install.ps1` download a GitHub Release asset from
  `PetarVukovic/knowlith`. Until a release is cut, use `scripts/dev.sh`
  or build the binary from source.
