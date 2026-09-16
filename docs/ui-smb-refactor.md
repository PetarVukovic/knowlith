# SMB UI/UX philosophy

Status: adopted for Simple mode (default).
Date: 2026-09-16

Knowlith’s product truth does not change: every claim has a quote, nothing
reaches an AI tool until a person approves it. This document only constrains
**how much of the machinery the owner has to see**.

Related: [`ARCHITECTURE.md`](../ARCHITECTURE.md),
[`competitive-pattern-architecture.md`](./competitive-pattern-architecture.md).

---

## One sentence

> The owner sees **one claim, where it came from, and what needs their OK**.
> Everything else stays under the hood until they ask.

There is **no knowledge-brain graph on Home**. The graph lives on
**Company brain** (`/brain`), rebuilt from approved objects, with Claude /
Codex / Cursor as launch ports. Everywhere else the graph appears only as
plain consequences: “If you change this, it also affects …”

---

## Primary user

The owner of a small or medium company (often non-technical). They care about:

1. Did my AI answer from *our* rules?
2. What is waiting on me to approve?
3. Where did this number / sentence come from?
4. Are my folders still being read?

They do **not** care about ObjectKind taxonomies, MCP, byte offsets, or
browsing forty-three atoms in a tree.

---

## Four surfaces (and only four)

| Surface | Route | Job |
| --- | --- | --- |
| **Today** | `/home` | Trust + attention: inbox count, folder health, recent AI reads |
| **Inbox** | `/review` | Approve / edit / discard — the daily job |
| **Browse** | `/browse` | Find a rule by search or filter — not a permanent tree |
| **One claim** | `/workspace/:id`, `/skills/:id` | Claim ∥ quote; suggest change; optional “also affects” |

Everything else is **Settings-tier**:

| Surface | Route | Notes |
| --- | --- | --- |
| Settings | `/settings` | Company name, appearance, detail level, read policy, login item |
| Sources | `/sources` | Health language; paths only in Engineer mode |
| AI assistants | `/connect` | Proof of connection and use |
| History | `/activity` | What was *read* — never why a model answered |

Cmd+K remains the fastest path into Browse.

---

## Navigation rules

### Sidebar

- **Flat links only.** No expandable Rules / Processes / Skills / Knowledge trees.
- Company mark + name at top.
- Badge only on Inbox when something waits.
- Width stays narrow; it is a map of *jobs*, not of *data*.

### Top bar

- Brand, search (Cmd+K), appearance.
- **Engineer mode is not a chrome toggle.** It lives under Settings → Detail
  level (and Cmd+K). Simple mode must not advertise “this is for engineers”.

### Status bar

- Sync / demo / work-in-progress / waiting-on-you — in plain language.
- Compiler stats only when Engineer mode is on.

---

## Claim detail (composition)

Default layout is **two regions in one page**, not three app columns:

```text
┌─────────────────────────────┬──────────────────────────┐
│ Title (human)               │ From your documents      │
│ The claim (markdown body)   │ Quote · verified         │
│ One status sentence         │ Open original            │
│ Suggest a change            │                          │
│ If you change this → …      │                          │
└─────────────────────────────┴──────────────────────────┘
```

- Evidence sits **beside or under** the claim — never behind a tab the owner
  might skip.
- Dependencies / history / raw JSON sit under **More**, or Engineer-only.
- No force-directed graph. Impact is a short sentence + links.

---

## Language (Simple mode)

| Avoid | Prefer |
| --- | --- |
| Knowledge Reference | The object’s own title |
| Changes | Inbox |
| Dependencies | If you change this |
| Synced / Watched / local-mcp | Last read · Folders are fine / needs attention |
| verified (alone) | Exactly as written in the document |
| Engineer mode (always visible) | Hidden under Appearance |

Owner-facing plurals stay generated in Rust where they already are. UI copy
stays English in-repo; company document content may be Croatian.

---

## Honesty constraints (do not “simplify away”)

These claims must remain checkable on screen:

- A quote is present and opens the source document.
- Unapproved knowledge is not implied to be live in AI tools.
- Demo / disconnected daemon is named, never dressed as a healthy company.
- Activity reports reads, not “why the assistant answered that”.
- No “Claude ✓ updated” — only “effective for conversations from now”.

Simplifying chrome is allowed. Softening the evidence gate is not.

---

## Anti-patterns

- Sidebar that lists every object (does not scale; trains the wrong browse habit).
- Three-pane IDE as the default for every route.
- Knowledge graph explorer as a product surface.
- Status pill festivals (four badges where one sentence would do).
- Paths, ids, and processor names in Simple mode source cards.

---

## Implementation checklist

- [x] Philosophy documented here
- [x] Flat sidebar (Today / Inbox / Browse / Folders / AI tools / Activity)
- [x] `/browse` list + filters (replaces tree)
- [x] Top bar: remove Engineer switch from chrome
- [x] Claim detail: evidence co-located; inspector tabs demoted
- [x] Sources: hide path / processor jargon in Simple mode
- [x] Rename “Changes” → “Inbox” in nav and headings where owner-facing

Engineer mode behaviour (ids, paths, raw JSON, compile stats) remains available;
it is no longer the shape of the product.
