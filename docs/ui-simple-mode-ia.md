# SMB Simple-mode IA

Status: adopted.
Date: 2026-09-16

Owner-facing chrome is English (repository rule). The mental model is the
Croatian product brief: three questions, four actions, no infrastructure
vocabulary in Simple mode.

Related: [`ui-smb-refactor.md`](./ui-smb-refactor.md), [`ARCHITECTURE.md`](../ARCHITECTURE.md).

---

## Three questions

1. **What does Knowlith know?**
2. **What changed?**
3. **Do I need to do something?**

## Four actions

```text
Add sources → Knowlith finds knowledge → Confirm changes → AI assistants use it
```

Everything else is secondary.

## Navigation (Simple)

| Label | Route | Job |
| --- | --- | --- |
| Home | `/home` | Trust + attention in five seconds |
| For review | `/review` | Confirm / decide / discard |
| Company knowledge | `/browse` | Search approved knowledge; type explained in plain language |
| Company brain | `/brain` | Live graph of confirmed knowledge + Ask AI |
| AI assistants | `/connect` | Connect Claude, Codex, Cursor… |
| Sources | `/sources` | Where files are read from |
| History | `/activity` | What was found, confirmed, and used |
| Settings | `/settings` | Company, appearance, reading policy |

## Kind language (owner-facing)

| Kind | Meaning shown in UI |
| --- | --- |
| Rule | A decision your company already made — limits, deadlines, who approves what |
| Process | How work is done here, step by step |
| Business term | A word or product name your company uses in its own way |
| AI skill | A task an assistant can carry out from confirmed knowledge |

Every approved item has **Try … in your AI**. Home has **Show me around** (arrow tour).

## Honesty

| Internal | Owner-facing |
| --- | --- |
| Inbox | For review |
| Browse | Company knowledge |
| AI tools | AI assistants |
| Folders | Sources |
| Activity | History |
| objects / edges | (hide) |
| Offered and not opened | Had access but did not use |
| Read · N | Used: … |
| Skills drafted… | Found N tasks AI can do for your team |
| Synced / Watched / MCP | Last read / Active / (Engineer only) |

## Home composition

1. Greeting + one status sentence (up to date / needs you / still reading).
2. Needs your attention (rules to confirm, disagreements, paused sources) + CTA.
3. Your company (counts with plain-language hints).
4. Connected AI assistants (short strip).
5. LiveWork / compiler lines **only in Engineer mode**.

## Honesty

Evidence, open questions, and “what was read vs why the model answered”
rules from `ARCHITECTURE.md` still hold. Plain language must not invent
acknowledgements or reasons the gateway cannot check.
