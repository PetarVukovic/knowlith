# Knowlith

Your company knowledge, compiled for every AI.

## The problem

Every AI assistant today is fluent and empty about *your* company.

Ask Claude, ChatGPT or Cursor how your firm handles a discount, a warranty or
a payment term and you get one of three answers: a confident guess from the
open web, a paraphrase of whatever happened to be in the chat context, or a
polite admission that it does not know. None of those is your price list. None
of them is the procedure your team actually follows.

Companies try to fix this by pasting folders into prompts, dumping PDFs into
retrieval indexes, or hoping the model will “remember” last week’s conversation.
That fails in the same ways every time:

- **Nothing is checked.** A sentence that sounds right is treated the same as
  a sentence that is written in the file.
- **Nothing is approved.** Yesterday’s draft and this year’s signed policy sit
  side by side, and the model picks whichever fragment retrieval returned.
- **Nothing is accountable.** When the assistant answers, you cannot see which
  document it read — or that it read nothing and answered anyway.
- **Figures drift.** A price remembered from a paragraph is not a price read
  from the row it lives in.

The result is an AI that sounds like it works for you while quietly inventing
the parts it never saw.

## Why this project exists

Knowlith is the missing layer between your shared drive and the AI tools you
already pay for.

It reads a folder on your machine, turns documents into knowledge that can be
traced back to the sentence they came from, and serves only what you have
approved — to Claude, Codex, Cursor and anything else that speaks MCP.

That is a deliberate refusal of three common lies:

- It will not tell you a rule is true unless a quote in your own file backs it.
- It will not tell you an assistant “updated” when the protocol cannot confirm
  that.
- It will not invent an ETA for work that depends on document size and a model
  it does not control.

Everything stays on `127.0.0.1`. Nothing is uploaded. The only copy of what you
approved lives under `~/Knowlith` on your computer.

## Install

One command. No administrator. Nothing leaves the machine.

```sh
# macOS and Linux
curl -fsSL https://raw.githubusercontent.com/PetarVukovic/knowlith/main/install.sh | sh
```

```powershell
# Windows
irm https://raw.githubusercontent.com/PetarVukovic/knowlith/main/install.ps1 | iex
```

Then:

```sh
knowlith serve            # open the interface on http://127.0.0.1:7717
knowlith connect          # hand approved knowledge to Claude, Codex, Cursor
knowlith autostart on     # keep working when the window is closed
```

In the interface: name the company, point it at your folder, review what it
found, approve what is true. Until you approve, AI tools see subjects — not
answers.

Removing Knowlith is deleting `~/Knowlith`, the binary, and whatever
`knowlith autostart off` and `knowlith disconnect` leave behind — which is
nothing.

## What you get

**A company brain you can open.** Rules, prices, procedures and vocabulary,
each with the passage they rest on.

**A review queue only you can clear.** Duplicates, conflicts and open questions
are named and held back. An undecided subject is never silently answered.

**A trail of what was actually read.** The Activity screen shows which tool
asked, what it opened, and what it skipped — not why the model phrased an
answer the way it did (that cannot be known).

**Figures from the row.** Prices and thresholds come out of the table they are
written in. They are not guessed from a nearby sentence.

## Who it is for

Owners and operators of a real company who already use AI day to day and need
it to stop improvising about their own rules — without sending the shared drive
to somebody else’s cloud.

## For people who change the code

The shape of the system is in [`ARCHITECTURE.md`](ARCHITECTURE.md).
How to work on it is in [`CLAUDE.md`](CLAUDE.md).
