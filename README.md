# Knowlith

Your company knowledge, compiled for every AI.

Knowlith reads a folder on your machine, turns documents into knowledge that can be traced back to the sentence it came from, and serves only what you have approved — to Claude, Codex, Cursor and anything else that speaks MCP.

Nothing reaches an AI tool until a person approves it. Nothing can be approved without the quote in your own file.

---

## The problem

Every AI assistant today is fluent and empty about *your* company.

Ask Claude, ChatGPT or Cursor how your firm handles a discount, a warranty or a payment term and you get one of three answers: a confident guess from the open web, a paraphrase of whatever happened to be in the chat context, or a polite admission that it does not know. None of those is your price list. None of them is the procedure your team actually follows.

Companies try to fix this by pasting folders into prompts, dumping PDFs into retrieval indexes, or hoping the model will “remember” last week’s conversation. That fails in the same ways every time:

- **Nothing is checked.** A sentence that sounds right is treated the same as a sentence that is written in the file.
- **Nothing is approved.** Yesterday’s draft and this year’s signed policy sit side by side, and the model picks whichever fragment retrieval returned.
- **Nothing is accountable.** When the assistant answers, you cannot see which document it read — or that it read nothing and answered anyway.
- **Figures drift.** A price remembered from a paragraph is not a price read from the row it lives in.

---

## What Knowlith does

```
Your folder  →  extract  →  compile  →  review queue  →  you approve  →  MCP gateway  →  Claude / Codex / Cursor
```

| Layer | What it means for you |
| --- | --- |
| **Extract** | PDF, DOCX, XLSX, CSV, Markdown — deterministic, no model. Same bytes, same offsets every time. |
| **Compile** | One model stage proposes rules, processes and terms — each with the sentence it came from. |
| **Evidence gate** | A quote that is not in the file is refused, whatever the model said. |
| **Review** | Duplicates, conflicts and open questions are held back until you decide. |
| **Brain** | A 3D map of approved knowledge and how it connects. Click a node, see its edges, ask in your connected AI. |
| **MCP gateway** | `get_task_context` returns rich, dependency-aware packs in one call — foundations, bodies and quotes together. |
| **Activity** | What was *read*, by which tool — not why the model phrased an answer the way it did. |

Knowlith runs only on your computer. Your documents are read by the AI you already pay for, through your own account. The only copy of what you approved lives under `~/Knowlith`.

---

## Install

One command. No administrator. No Knowlith account, no Knowlith server.

```sh
# macOS and Linux
curl -fsSL https://raw.githubusercontent.com/PetarVukovic/knowlith/main/install.sh | sh
```

```powershell
# Windows
irm https://raw.githubusercontent.com/PetarVukovic/knowlith/main/install.ps1 | iex
```

The installer downloads a single binary (UI embedded — no Node at runtime) and places it on your `PATH`. Binaries for macOS (Apple Silicon and Intel), Linux and Windows are on the [Releases](https://github.com/PetarVukovic/knowlith/releases) page.

**Contributors:** build from source with `cargo build --release --bin knowlith` or use `sh scripts/dev.sh` for hot reload.

---

## Quick start

```sh
knowlith start            # daemon + worker + open browser
knowlith connect          # hand approved knowledge to Claude, Codex, Cursor
knowlith connect --open    # same, and open the first connected app
knowlith autostart on     # keep working when the window is closed
knowlith export           # write approved knowledge to ~/Knowlith/knowledge/
```

**First run in the browser:** name the company → pick a folder → review what was found → approve what is true → connect an AI tool.

Until you approve, AI tools see subjects — not answers.

### From source (one command)

When building from this repository, use `scripts/start.sh` — it starts the daemon with an explicit reader, applies the same settings the UI would save (via curl), and opens the interface fullscreen:

```sh
sh scripts/start.sh --fresh --demo
```

| Flag / env | Meaning |
| --- | --- |
| `--fresh` | Move aside the existing `~/Knowlith` and start empty |
| `--demo` | Add `~/Documents/knowlith-demo/invoices` and enqueue a read |
| `KNOWLITH_ENGINE=cursor-agent` | Reader CLI (default: `cursor-agent`; also `codex`, `claude-code`) |

Supervisor demo end-to-end (reset, compile, build quiz): `sh scripts/fresh-start.sh`

**Changing the reader in Settings only takes effect after you restart Knowlith.** The background worker binds its engine at daemon start; curl and the UI persist the choice for the next start.

---

## Test the running daemon (curl)

With Knowlith listening on `127.0.0.1:7717`:

```sh
TOKEN=$(cat ~/Knowlith/api.token)

# Health
curl -sf -H "x-knowlith-token: $TOKEN" http://127.0.0.1:7717/api/health

# Company name
curl -sf -H "x-knowlith-token: $TOKEN" http://127.0.0.1:7717/api/company

# Sources and review queue
curl -sf -H "x-knowlith-token: $TOKEN" http://127.0.0.1:7717/api/sources
curl -sf -H "x-knowlith-token: $TOKEN" http://127.0.0.1:7717/api/review

# Background work
curl -sf -H "x-knowlith-token: $TOKEN" http://127.0.0.1:7717/api/work

# Company brain graph
curl -sf -H "x-knowlith-token: $TOKEN" http://127.0.0.1:7717/api/brain

# Export approved objects to Markdown
curl -sf -H "x-knowlith-token: $TOKEN" -X POST -H "Content-Type: application/json" \
  -d '{}' http://127.0.0.1:7717/api/export
```

Full smoke test: `sh scripts/curl-test.sh`

End-to-end from empty machine: `sh scripts/full-test.sh`  
MCP gateway proof: `sh scripts/verify-gateway.sh`

---

## MCP tools (what your AI sees)

Primary workflow:

1. **`get_task_context`** — full pack: bodies, evidence quotes, foundations, case id
2. **`lookup_value`** — every figure from the company's own table row
3. **`check_coverage`** — what was offered but never read

Also: `get_relevant_context`, `get_context`, `search_context`, `get_skill`, `get_process`, `lookup_value`, `list_pending`, `propose_change`, and more.

See [`docs/mcp-rich-context.md`](docs/mcp-rich-context.md) for the rich context layer.

---

## What you get in the interface

- **Onboarding wizard** — one decision per screen; Home stays closed until a folder is read and the first review path finishes.
- **Review queue** — only you can clear it. An undecided subject is never silently answered. Missing source files surface as their own review rows — not as silent approval.
- **Build quiz** — after the build supervisor reads a whole folder, confirm its synthesis of company rules before they become knowledge (`/build-quiz`).
- **Company brain** — 3D graph; click to highlight connections; double-click to open; ask in Claude Desktop, Codex or Terminal via `Try in your AI`.
- **Sources** — folders watched with write-settle and filesystem events; periodic rescan as safety net; per-file index shows which approved objects still quote each snapshot.
- **Activity** — which tool asked, what it opened, what it skipped.
- **Portable export** — `~/Knowlith/knowledge/` mirrors approved objects as Markdown you can back up or move.

---

## Development

```sh
sh scripts/start.sh --fresh --demo   # production UI on 7717, curl setup, fullscreen browser
sh scripts/dev.sh                    # daemon on 7717 + Vite on 5173 (hot reload)
sh scripts/dev.sh --fresh            # same, from an empty company
KNOWLITH_ENGINE=codex sh scripts/dev.sh   # override reader (default: cursor-agent)
cargo test --workspace               # ~370 tests
```

Reset local install: `sh scripts/reset.sh --yes`

Architecture: [`ARCHITECTURE.md`](ARCHITECTURE.md)  
Contributing: [`CLAUDE.md`](CLAUDE.md)

---

## Who it is for

Owners and operators of a real company who already use AI day to day and need it to stop improvising about their own rules — without handing the shared drive to a new vendor.

---

## Remove Knowlith

```sh
sh scripts/reset.sh --yes
```

Deletes `~/Knowlith`, disconnects AI apps, removes autostart, and removes the binary from `~/.local/bin`.
