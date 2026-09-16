# Where this stands

The gateway is built. A folder becomes knowledge, the owner approves it, and
an AI tool on the same machine can read it — with the document, the locator
and the quote attached to every answer.

## Done this round

- Reads are attributed to the application that asked. `clientInfo` is read
  in `initialize`, normalised in one function with its own tests, and
  carried on every `tool_reads` row and every case.
- **AI tools** separates connection from use: "Connected" and "Read 3
  things · last 8 min ago" are two claims, on two lines.
- **Activity** — one row per question, what was read, and what the agent was
  offered and never opened.
- "Try your company context" builds its questions from this company's own
  approved objects and then watches the gateway until something is served.

- Adding a folder from the interface. `POST /api/sources` records it and
  queues a walk the running worker picks up; `POST /api/sources/browse`
  opens the machine's own chooser, because a browser is never told where a
  folder is; `GET /api/sources/preview` counts one without opening anything.
- `knowlith-extract::inventory` — files, size, readable types, duplicates,
  names that look like last year's copy, and the types that cannot be read.
  The same two rules the real scan uses, so the count shown is the count
  that will be read.
- `scripts/dev.sh` — daemon and interface in one command.
- Onboarding reads from the daemon end to end. The browser-side inventory
  and the demo folder behind it are gone.

- `crates/mcp` — JSON-RPC over stdio, 11 tools, skills as prompts, the
  company card as a resource, `listChanged` when the owner approves
  something mid-conversation, a panic in one tool refused rather than fatal.
- `crates/desktop` — every path, application config, login registration,
  power reading and the company's icon, in one crate so the Windows story is
  four files rather than eleven thousand lines.
- Claude Desktop, Claude Code and Codex connected in place, with a backup,
  atomically, preserving comments and everything else in the file.
- `knowlith.mcpb`, so Claude Desktop can install it with its own screen.
- Standing instructions written into `AGENTS.md` and `CLAUDE.md`, inside
  markers, so the agent knows when to reach for the company.
- The background service registered with `launchd` / Task Scheduler /
  `systemd --user`, with a policy for what it may spend on its own.
- The interface compiled into the binary; `curl | sh` and `irm | iex`
  installers; a release workflow that builds and tests four targets.

## Next, in order

1. **Run it on a real Windows machine.** Everything is written for it and
   the tests run there in CI, but the scheduled task, Claude Desktop's
   install path and `claude.cmd` on `PATH` have never met a real desktop.

2. **Per-application read counts.** `tool_reads` records every serve, but
   the protocol does not carry the client's identity, so "Claude read your
   pricing 5 times today" cannot honestly be said yet. The client's name
   arrives in `initialize`; carrying it through to each read is small.

3. **The activity feed.** The data is already there — `tool_reads`,
   `cases`, approvals, job outcomes. What is missing is one screen that
   turns it into "Codex used *Odobravanje popusta* for a quote", which is
   what makes an owner feel the thing is alive.

4. **Knowlith Managed.** `ManagedEngine` still says it does not exist.

## Decisions that were made

- **An agent may write, as `proposed` only.** `propose_change` goes through
  the same evidence gate as the compiler: the quote is located in a real
  document or the suggestion is refused. There is no second path to
  becoming company knowledge.
- **Skills are prompts and tools.** A prompt so the owner can type one; a
  tool so an agent mid-task can reach for one. Same object.
- **Neither preload nor search alone.** `get_relevant_context` returns the
  map of what is relevant, which is small, always accurate, and forces the
  retrieval rather than replacing it. The company card stays as a resource
  for clients that attach one.
