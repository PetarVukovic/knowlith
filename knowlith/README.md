# Knowlith

> Your company knowledge, compiled for every AI.

React UI for Knowlith: the screens an owner and an engineer use to turn a folder of
company documents into a reviewed, evidence-backed context that AI agents read.

This package is the interface only. The compiler and the file access live in the Rust
daemon, which is a separate crate workspace.

## Running it

```bash
npm install
npm run dev      # http://localhost:5173
npm run build    # tsc -b && vite build
npm run lint     # oxlint
```

## Where the data comes from

`src/lib/api.ts` is the single module that knows the data source. Today it resolves the
demo fixtures in `src/lib/mock.ts`. When the daemon runs, each function becomes a `fetch`
against `127.0.0.1` and nothing else in the UI changes.

`src/lib/types.ts` is the contract. Every type there is something the daemon can actually
produce — in particular `Evidence`, which carries a document id, a byte range and the
verbatim quote at that range. The UI never displays a claim it cannot trace back to one.

## Screens

| Route | What it is for |
| --- | --- |
| `/onboarding` | Company, source folder, processor, explicit permission, first build. |
| `/discovery` | What the first compile found, and what is waiting for a person. |
| `/workspace`, `/workspace/:objectId` | The canonical document, with an inspector for evidence, dependencies and history. |
| `/review` | Before/after diff, source evidence beside it, and what approving will change. |
| `/sources` | Folders and NAS roots: access, last read, processor, pause / re-read / remove. |
| `/skills/:skillId` | The full SKILL.md, what it is built on, its inputs and outputs. |

## Two modes

`simple` is the default and hides everything an owner should never need: byte offsets,
object ids, raw JSON, compiler runs, numeric confidence. `engineer` turns all of it on.
The toggle lives in the top bar and in the command palette (`⌘K`).

Confidence is deliberately rendered as language in simple mode. The number is calibrated
from source agreement rather than a model's self-report, and showing `0.83` to a
non-technical owner invites a precision the number does not have.

## Design

- Linear for spacing, type and command-driven navigation; VS Code only for the left-hand
  explorer model; Notion for the calm document surface; GitHub for the diff and approval
  flow; Raycast for compact controls and onboarding.
- Tokens in `src/index.css` under `:root` and `.dark`, exposed to Tailwind v4 through
  `@theme inline`. Components never hardcode a colour.
- One accent (teal). Semantic colours — pending, conflict, confirmed — are separate from
  it, so "this needs attention" never reads as "this is the brand".
- No gradients, no glassmorphism, no bento grid, no invented analytics.

## Not implemented here

Real file access, the compiler, the MCP gateway and the permission enforcement all belong
to the daemon. The UI assumes the daemon rejects anything it should not serve; it does not
enforce policy on its own.
