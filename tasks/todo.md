# Next: the MCP gateway

The last unbuilt piece of the loop. Everything up to it works: a folder is read,
compiled, settled across documents, related into a graph, drafted into skills and
approved in the interface — and then the approved knowledge sits in SQLite and
reaches nothing. `tool_reads` is an empty table, which is why no screen says
"read by 3 AI tools": it would not be true.

## Shape

A stdio server the client spawns, not an HTTP port. Same pattern as the engine:
the client owns the process lifetime, the boundary is the user account on this
Mac, and there is no network and no authentication to invent.

```json
{ "mcpServers": { "knowlith": { "command": "knowlith", "args": ["mcp"] } } }
```

It opens its own connection to `lake.sqlite` — a third one, next to `server` and
`worker`. WAL is what makes that safe, and not sharing a mutex is what keeps a
long agent call from freezing the interface.

Read-only, with exactly one exception: it writes `tool_reads`.

## The gate

This is the product decision, not a technical one.

| Object state | What the agent gets |
| --- | --- |
| `approved` + at least one verified span | the text, the quote and the source |
| `approved` + `stale_since` set | the same, flagged: the source moved after approval |
| `conflicted` | the *name* of the open question, never either answer |
| `proposed` | the same — named, not given |
| `superseded`, `rejected` | nothing at all |

The middle rows matter more than they look. Hiding an undecided question does
not stop an agent answering it — it makes the agent invent an answer. Told
"there is an open question about payment terms", an agent says so and asks the
owner. Told nothing, it writes 30 days because that sounds right.

Serving a stale answer silently and withholding one silently are both worse than
saying which it is.

## Tools

| Tool | Answers | Why it is separate |
| --- | --- | --- |
| `search_knowledge` | "what does this company say about complaints" | FTS5, diacritics folded, approved objects only |
| `get_rules` | the preload at the start of a conversation | the small card that always applies: VAT, terms, currency |
| `read_source` | "show me the exact passage" | so the agent quotes verbatim instead of paraphrasing |
| `lookup_value` | "what does installing three units cost" | an exact row read, never similarity |
| `what_breaks_if` | "I am changing the turnover threshold" | the graph — the one thing nothing else here has |
| `what_this_rests_on` | attached to every rule returned | a discount rule without its threshold is incomplete |
| `list_pending` | "what has the owner not decided" | so the agent knows when it must not answer |

A figure is never returned from prose. Price rows were dropped from the approval
queue for this reason: a price is something the daemon reads, not something a
model recalls. Retrieved by similarity a price comes back *close*; read from the
row it comes back right or not at all.

Every result carries its document name, locator and quote, so the agent's answer
can cite. Every serve writes a `tool_reads` row.

## Decisions the owner has to make first

1. **May an agent write back?** The proposal is yes, but only as `proposed`, so
   anything an agent suggests lands in the same review queue as everything the
   compiler produces. No second path to becoming knowledge. The alternative is
   strictly read-only.
2. **Skills as MCP prompts or as tools?** Prompts give `/knowlith:odobri-popust`
   in Claude Code, which feels far better; tools let the agent reach for one
   itself. Probably both surfaces over the same object.
3. **Preload or search only?** Fifty-one objects fit in a context window and
   three thousand do not. A small always-loaded company card plus search for
   everything else — but what belongs on that card is a product decision, not a
   technical one.

## After that

- Knowlith Managed. `ManagedEngine` currently says it does not exist rather than
  pretending, which is the right placeholder but not a product.
- Amounts written in words are not recognised as figures.
