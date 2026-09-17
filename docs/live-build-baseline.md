# Live build baseline — Medicor demo, 2026-09-17

Measured on one live run of `~/.local/bin/knowlith` **0.1.3** against
`/Users/petarvukovic/Desktop/Medicor-Klinika-Demo`, engine `cursor-agent`,
`pauseOnBattery: false`, `compileWorkers: 2`, `compileBatchSize: 8`,
`relateAfterBuild: true`. Diagnosis is the lake (`jobs`, `engine_runs`) and
`GET /api/work`. `knowlith start` does not write a file log under `~/Knowlith`.

These numbers are a floor, not a forecast. The product still refuses to
estimate compile time; this file exists so later work has a corpus size and
a place the minutes actually went.

## Folder size

| | |
| --- | --- |
| Path | `~/Desktop/Medicor-Klinika-Demo` |
| Files on disk | 24 (walker reported 23; `.DS_Store` ignored) |
| Bytes on disk | **42 806 B (41.8 KiB)** |
| Extracted text in lake | **6 711 B across 17 documents** |
| Lake file | `~/Knowlith/data/lake.sqlite` 404 KiB |

On-disk by type:

| Type | Files | Bytes |
| --- | ---: | ---: |
| `.py` (generator, not company knowledge) | 1 | 13 942 |
| `.pdf` | 5 | 10 489 |
| `.DS_Store` | 1 | 6 148 |
| `.xlsx` | 1 | 5 270 |
| `.md` | 9 | 3 690 |
| `.json` | 3 | 1 897 |
| `.sh` | 1 | 463 |
| `.txt` | 1 | 456 |
| `.csv` | 2 | 451 |

Largest company files are still tiny: `Cjenik-2026.xlsx` 5.3 KiB, the
readable PDFs 2.0–2.6 KiB each. This is a toy SMB folder. Wall time and
token spend below are **not** explained by bytes on disk.

## What the run produced

| | |
| --- | --- |
| Walked / stored / unreadable | 23 / 17 / **6** |
| Candidates → objects | 75 → 45 kept (10 dropped, 5 conflicts, 8 merge pairs) |
| Relations written | 10 edges (24 rows in `relations`) |
| Engine | Cursor Agent, **152 617 tokens** before supervise finished (101 736 in + 50 881 out). `costUsd` was null. |

Six paths never became documents (walker: not readable):

- `04-pacijenti/partneri-osiguravatelji.json`
- `05-interno/Kontakti-dobavljaci.json`
- `07-laboratorij/rezultati-template.json`
- `06-skenirano/Ugovor-najam-prostorija.pdf` (1 542 B)
- `generate_demo.py`
- `test-medicor.sh`

JSON is company data (partners, supplier contacts) that the extract crate
does not ingest. The lease PDF is in the folder and absent from the lake.

## Where the minutes went

Wall clock from first rescan (`14:10:04Z`) to relate done (`14:15:35Z`) is
**5 min 31 s**. Supervise was still leased after that (session `build:23`,
Cursor Agent child alive, attempts already at 2).

| Job | Wall | Tokens (in/out) | Note |
| --- | ---: | --- | --- |
| rescan | 0.5 s | — | Cheap. Second scan 72 ms. |
| compile (3 CLI batches, 2 workers) | **~2.7 min** | 63 086 / 31 496 | 17 jobs; overlapping leases, so per-row `finished - created` looks like 98–160 s each. |
| settle | **17 ms** | — | Deterministic. Never the stall. |
| **relate** | **171.6 s** | **38 650 / 19 385** | More output tokens than any compile batch. UI: “Preparing skills and connections”. |
| supervise | **still running at capture** | not yet in `engine_runs` | Second whole-folder CLI pass (entities + canonical + quiz). UI: “Building company knowledge from your folder”. |

Compile batches:

1. `RAC-2026-002-skena.pdf and 7 more` — 24 438 / 11 523
2. `RAC-2025-014.md and 6 more` — 20 355 / 13 610
3. `Cjenik-usluga-2026.md and 1 more` — 18 293 / 6 363

## Problems to attack later

Ordered by what made this 42 KiB folder feel stuck, not by architectural purity.

1. **Relate is a full-corpus model call and currently outruns compile.**
   58 k tokens and 2 min 51 s to draw 10 edges over 45 objects. Dependency
   is a property of the set, so one pass is correct — the pass is too fat
   for a corpus whose extracted text is 6.7 KiB.

2. **`relateAfterBuild: true` did not defer relate.** Policy was on; relate
   still ran to completion *before* supervise started. `may_enqueue_relate`
   returns true when there is no quiz yet and `build_phase` is unset, so
   settle → relate → supervise. The flag’s comment says the supervisor
   should not sit behind a whole-folder model pass. This run did exactly
   that.

3. **Supervise is a second whole-folder CLI after relate already paid.**
   Same engine, overlapping job (canonical rules vs edges). Two cold
   `cursor-agent` starts, two giant prompts. Capture showed `attempts: 2`
   and `turn_count: 2` while still leased — retries or extra turns on a
   hung child will multiply the cost.

4. **CLI cold start dominates a tiny corpus.** Three compile batches for
   6.7 KiB of text. Batching helped (8+7+2) but each batch is still a
   process. A real clinic folder will need this measured again; the
   shape to watch is *tokens per extracted kilobyte* and *CLI invocations
   per folder*, not files walked.

5. **Work panel looks idle while the child is thinking.** Relate and
   supervise are one leased row with note `started`. There is no live
   token/stream line. The owner sees a spinner. That is honest (the job
   is running) and unusable (they cannot tell relate from a hang). A
   later panel should show engine + elapsed + last `engine_runs` row,
   never a fake ETA.

6. **No stderr log for `knowlith start`.** Health checks have to query
   SQLite. Worth a rotating `~/Knowlith/daemon.log` for engine spawn,
   lease renew, and classify() failures — not a second source of truth
   for counts.

7. **JSON (and similar) is walked then dropped.** Three files with
   partners and contacts never compile. Either ingest them or do not
   count them as “not readable” next to scanned PDFs. Same for
   `generate_demo.py` / `test-medicor.sh` sitting inside a company
   source.

8. **Tiny / empty PDFs still take a compile slot.**
   `Ugovor-najam-prostorija.pdf` (1.5 KiB) was unreadable;
   `Lab-rezultati-Ana-Horvat.pdf` compiled to **0 claims**. A scan with
   no text layer should say so on the source, not occupy a CLI batch.

9. **Cursor Agent does not report USD.** Spend UI can show tokens only.
   Pricing for paid people cannot be derived from this engine until the
   CLI JSON includes cost.

## What not to “optimise”

- Settle, rescan, recheck — already noise on this corpus.
- Inventing a compile ETA from this one run.
- Sending the lake to an external RAG cloud to make the spinner move.
- Vector index. FTS + graph still come after this bottleneck (the bottleneck
  is CLI tokens, not search).

## How to re-measure

```sh
curl -sf -H "x-knowlith-token: $(cat ~/Knowlith/api.token)" \
  http://127.0.0.1:7717/api/work | python3 -m json.tool

python3 -c "import sqlite3,os; c=sqlite3.connect(os.path.expanduser('~/Knowlith/data/lake.sqlite'))
print(c.execute('select kind,state,note,created_at,finished_at from jobs').fetchall())
print(c.execute('select stage,input_tokens,output_tokens,subject from engine_runs').fetchall())"
```

A later pass should add: wall time for supervise, quiz question count, and
the same table against a folder that is actually large (tens of MB of PDF),
so we know which of (2) and (4) survive.

## Fixed in 0.1.5

Shipped after this capture, against the bugs that made the 42 KiB folder look
stuck rather than against token spend:

- **Lease generation.** A heartbeat from the previous holder no longer
  extends a reclaimed job. Losing the lease cancels the CLI child, so a
  second `cursor-agent` does not sit next to a hung first one. (Problems 3
  and the hang that followed this capture.)
- **`relateAfterBuild` waits for a quiz.** An unset `build_phase` and no
  quiz is no longer treated as permission to relate. Supervise runs first.
  (Problem 2.)
- **Work panel.** A leased job says `running` / `Cursor Agent · 3 min`,
  never a frozen `started`. No ETA. (Problem 5.)
- **`~/Knowlith/logs/daemon.log`.** Serve, supervise spawn, and lost-lease
  lines go here even when `knowlith start` has no terminal. (Problem 6.)
- **JSON is read as text.** Partner and contact files are no longer counted
  as unreadable next to scanned PDFs. Secret names (`credentials.json`)
  still skip. (Problem 7.)

Still open from the list above: relate prompt size (1), CLI cold start (4),
empty-scan PDFs occupying a compile slot (8), Cursor Agent USD (9).

## 0.1.6

Claude Code children are serialised (the CLI races concurrent processes);
Cursor and Codex are not. A dropped wait kills the child. Stdout is
journalled under `data/runs/{job}/` as the CLI speaks, so a lost lease
still leaves a session on disk. Curl: `sh scripts/curl-test.sh`.
