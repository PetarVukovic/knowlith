import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { useNavigate } from "react-router-dom"
import {
  AlertTriangle,
  ArrowRight,
  Check,
  Copy,
  Download,
  ExternalLink,
  Loader2,
  MessageSquare,
  Terminal,
  Wrench,
} from "lucide-react"
import { Button } from "@/components/ui/button"
import { scheduleProductTour } from "@/components/ProductTour"
import { tools as toolsApi } from "@/lib/api"
import type { AiTool, ConnectPreview, Usage } from "@/lib/types"
import { formatRelative } from "@/lib/utils"
import { useApp } from "@/state/AppState"

/**
 * Handing the company to the tools the team already uses.
 *
 * Two decisions shape this screen.
 *
 * **Nothing is written before it is shown.** Pressing Connect opens what
 * would be added to a file the owner did not create, and a second press
 * agrees to it. The step costs three seconds and removes the whole class of
 * "what did that just do to my configuration".
 *
 * **It does not end at Done.** An owner who has connected something and
 * closed the window has no idea whether it worked. So the last thing here is
 * a question to ask, in their own company's words, with a button that opens
 * the application already running.
 */

const ICONS: Record<string, typeof Terminal> = {
  "claude-desktop": MessageSquare,
  "claude-code": Terminal,
  codex: Terminal,
  cursor: Wrench,
}

/** What each state means, in words rather than as a colour. */
function describe(tool: AiTool): { label: string; tone: string } {
  switch (tool.state) {
    case "connected":
      return { label: "Connected", tone: "text-confirmed" }
    case "needs-attention":
      return { label: "Needs attention", tone: "text-pending" }
    case "ready":
      return { label: "Found on this computer", tone: "text-muted" }
    default:
      return { label: "Not installed", tone: "text-faint" }
  }
}

/**
 * Whether this application has actually used the company's knowledge.
 *
 * Being in a configuration file proves somebody pressed a button. This
 * proves a conversation reached the company — which is the thing the owner
 * is really asking about, and the thing a green badge never answers.
 */
function Use({ tool }: { tool: AiTool }) {
  if (tool.reads === 0) {
    return (
      <span className="block text-[12.5px] text-muted">
        Has not read anything yet
      </span>
    )
  }
  return (
    <span className="block text-[12.5px] text-confirmed">
      Read {tool.reads} {tool.reads === 1 ? "thing" : "things"}
      {tool.lastRead ? ` · last ${formatRelative(tool.lastRead)}` : ""}
    </span>
  )
}

/** The verb on the button, which is never just "Connect". */
function verb(tool: AiTool, company: string): string {
  if (tool.state === "needs-attention") return "Repair"
  if (tool.state === "connected") return tool.running ? "Open" : "Open"
  if (tool.slug === "claude-desktop") return "Install in Claude"
  const name = company.trim() && company !== "Your company" ? company : "company knowledge"
  return `Add ${name}`
}

export function Connect() {
  const navigate = useNavigate()
  const { firstRun, setFirstRun, companyName, objects } = useApp()

  const [tools, setTools] = useState<AiTool[] | null>(null)
  const [preview, setPreview] = useState<{ slug: string; body: ConnectPreview } | null>(null)
  const [busy, setBusy] = useState<string | null>(null)
  const [note, setNote] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)

  const refresh = useCallback(async () => {
    setTools(await toolsApi.list())
  }, [])

  useEffect(() => {
    void refresh()
  }, [refresh])

  const begin = async (tool: AiTool) => {
    if (tool.state === "connected") {
      // CLI assistants have no desktop window — Open on Connect used to call
      // the daemon, get "not installed", and contradict the Connected badge.
      if (tool.launchSurface === "terminal") {
        setNote(`${tool.label} answers in the chat beside the company brain.`)
        navigate(`/brain?agent=${encodeURIComponent(tool.slug)}`)
        return
      }
      setBusy(tool.slug)
      // Open with a ready question in the composer — empty Open left owners
      // staring at a blank chat with no reason to reach for Knowlith.
      const prompt =
        objects.find((o) => o.status === "approved") != null
          ? `What does ${companyName} say that I should check before I answer a customer?`
          : `Using the ${companyName} knowledge server, what has been approved so far?`
      const result = await toolsApi.try(tool.slug, prompt)
      setNote(result && "message" in result ? result.message : (result?.error ?? null))
      setBusy(null)
      void refresh()
      return
    }
    const body = await toolsApi.preview(tool.slug)
    if (body) setPreview({ slug: tool.slug, body })
  }

  const allow = async (slug: string) => {
    setBusy(slug)
    setPreview(null)
    const result = await toolsApi.connect(slug)
    if (result?.connected) {
      // Claude Desktop reads its settings once, at launch. Restarting it is
      // the difference between "connected" being true and being useful, and
      // the owner should not have to be told that it is.
      const opened = await toolsApi.open(slug)
      setNote(opened?.message ?? result.refreshHint)
    } else {
      setNote("That did not go through. The panel below has what to paste by hand.")
    }
    setBusy(null)
    void refresh()
  }

  const installExtension = async () => {
    setBusy("bundle")
    const built = await toolsApi.bundle()
    setNote(
      built
        ? `Extension built (${built.megabytes} MB). Claude Desktop will show what it installs before it does.`
        : "The extension could not be built.",
    )
    setBusy(null)
  }

  const copy = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text)
      setCopied(true)
      window.setTimeout(() => setCopied(false), 2000)
    } catch {
      /* clipboard blocked — the text is on screen either way */
    }
  }

  const anyConnected = (tools ?? []).some((tool) => tool.connected)

  // Connecting anything finishes the first-run gate — Home stops bouncing.
  useEffect(() => {
    if (firstRun === "connect" && anyConnected) {
      setFirstRun(null)
    }
  }, [firstRun, anyConnected, setFirstRun])

  return (
    <div className="mx-auto w-full max-w-[720px] px-5 py-10">
      {firstRun === "connect" ? (
        <div className="mb-6 flex flex-wrap items-center justify-between gap-3 rounded-xl border border-accent/30 bg-accent-soft px-4 py-3">
          <p className="text-[13px] text-ink">
            Connect an assistant below, or open Home and come back later.
          </p>
          <Button
            size="sm"
            variant="primary"
            onClick={() => {
              setFirstRun(null)
              scheduleProductTour()
              navigate("/home")
            }}
          >
            Open Home
            <ArrowRight />
          </Button>
        </div>
      ) : null}

      <h1 className="text-[24px] font-semibold leading-tight tracking-[-0.022em] text-ink">
        AI assistants
      </h1>
      <p className="mt-2.5 max-w-[52ch] text-[14px] leading-relaxed text-muted">
        Connect Claude, Codex, Cursor or other assistants so they answer from what {companyName}{" "}
        approved — and cite the document it came from.
      </p>

      <div className="mt-8 space-y-2.5">
        {tools === null ? (
          <p className="text-[13px] text-faint">Looking at what is installed…</p>
        ) : (
          tools.map((tool) => {
            const Icon = ICONS[tool.slug] ?? Terminal
            const state = describe(tool)
            const working = busy === tool.slug

            return (
              <div key={tool.slug} className="rounded-xl border border-line bg-surface p-4">
                <div className="flex items-center gap-3">
                  <span className="grid size-9 shrink-0 place-items-center rounded-lg bg-surface-2 text-muted">
                    <Icon className="size-4" />
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="block text-[14px] font-medium text-ink">{tool.label}</span>
                    <span className={`block text-[12.5px] ${state.tone}`}>{state.label}</span>
                    {tool.installed && tool.launchSurface !== "missing" ? (
                      <span className="block text-[11.5px] text-faint">
                        {tool.launchSurface === "terminal"
                          ? "Opens in Terminal on this Mac"
                          : "Opens as a desktop app"}
                      </span>
                    ) : null}
                    {/* Connection and use are two different claims, and only
                        the second one means the product is working. They get
                        their own line each rather than being joined by a
                        dot, because "Connected · 0 reads" reads as success.
                        Use is shown even when the entry has gone missing:
                        an application that read this company last week and
                        is no longer connected is exactly the case the owner
                        most needs to see. */}
                    {tool.connected || tool.reads > 0 ? <Use tool={tool} /> : null}
                  </span>

                  {tool.installed ? (
                    <Button
                      size="sm"
                      variant={tool.state === "connected" ? "subtle" : "default"}
                      disabled={working}
                      onClick={() => void begin(tool)}
                    >
                      {working ? (
                        <Loader2 className="animate-spin" />
                      ) : tool.state === "connected" ? (
                        <ExternalLink />
                      ) : tool.state === "needs-attention" ? (
                        <Wrench />
                      ) : (
                        <Check />
                      )}
                      {verb(tool, companyName)}
                    </Button>
                  ) : (
                    <span className="shrink-0 text-[12px] text-faint">Not installed here</span>
                  )}
                </div>

                {tool.problem ? (
                  <p className="mt-3 flex items-start gap-2 rounded-lg bg-pending-soft p-2.5 text-[12.5px] text-pending">
                    <AlertTriangle className="mt-px size-3.5 shrink-0" />
                    <span>
                      {tool.problem} Repairing points it back at this one; nothing else in that file
                      is touched.
                    </span>
                  </p>
                ) : null}

                {tool.slug === "claude-desktop" && tool.installed && tool.connected ? (
                  // Claude runs local servers in its chat window only. A
                  // Cowork session gets remote connectors and nothing from
                  // this machine, and an owner who asks there and hears a
                  // guess has no way of knowing why.
                  <p className="mt-3 text-[12px] text-muted">
                    Readable in Claude Desktop chat. Cowork sessions cannot reach servers on this Mac, so
                    ask in the chat.
                  </p>
                ) : null}

                {tool.slug === "claude-desktop" && tool.installed ? (
                  <div className="mt-3 flex flex-wrap items-center gap-2.5 border-t border-line pt-3">
                    <span className="flex-1 text-[12px] text-muted">
                      Or install it as a Claude Desktop extension, which shows you what it may reach
                      before you enable it.
                    </span>
                    <Button
                      size="sm"
                      variant="subtle"
                      disabled={busy === "bundle"}
                      onClick={() => void installExtension()}
                    >
                      {busy === "bundle" ? <Loader2 className="animate-spin" /> : <Download />}
                      Build extension
                    </Button>
                  </div>
                ) : null}

                {preview?.slug === tool.slug ? (
                  <div className="mt-3 rounded-lg border border-line bg-surface-2 p-3">
                    <p className="text-[12.5px] text-muted">
                      This will be added to{" "}
                      <span className="font-mono text-[11.5px] text-ink">
                        {preview.body.configPath}
                      </span>
                      . Everything already in that file stays, and a copy of it is kept.
                    </p>
                    <pre className="scroll-thin mt-2.5 overflow-x-auto rounded-md border border-line bg-surface p-2.5 font-mono text-[11.5px] leading-relaxed text-muted">
                      {preview.body.snippet}
                    </pre>
                    <div className="mt-3 flex flex-wrap items-center gap-2">
                      <Button size="sm" variant="primary" onClick={() => void allow(tool.slug)}>
                        Allow
                      </Button>
                      <Button size="sm" variant="ghost" onClick={() => setPreview(null)}>
                        Cancel
                      </Button>
                      <button
                        type="button"
                        onClick={() => void copy(preview.body.snippet)}
                        className="ml-auto inline-flex items-center gap-1.5 text-[12px] text-faint underline-offset-4 transition-colors hover:text-muted hover:underline"
                      >
                        {copied ? <Check className="size-3" /> : <Copy className="size-3" />}
                        {copied ? "Copied" : "I'll do it myself"}
                      </button>
                    </div>
                  </div>
                ) : null}
              </div>
            )
          })
        )}
      </div>

      {note ? (
        <p className="mt-4 rounded-lg border border-line bg-surface-2 p-3 text-[12.5px] text-muted">
          {note}
        </p>
      ) : null}

      {anyConnected ? <TryIt company={companyName} /> : null}

      {firstRun === "connect" ? (
        <div className="mt-9 flex flex-wrap items-center gap-3">
          <Button
            size="lg"
            variant="primary"
            onClick={() => {
              setFirstRun(null)
              scheduleProductTour()
              navigate("/home")
            }}
          >
            Open company home
            <ArrowRight />
          </Button>
          <button
            type="button"
            onClick={() => {
              setFirstRun(null)
              scheduleProductTour()
              navigate("/home")
            }}
            className="text-[12px] text-faint underline-offset-4 transition-colors hover:text-muted hover:underline"
          >
            I'll connect a tool later
          </button>
        </div>
      ) : null}
    </div>
  )
}

/**
 * The first question, and the proof that it landed.
 *
 * Connecting something and being told it worked proves nothing. This copies
 * a question about the company's own approved knowledge, then watches the
 * gateway until that knowledge is actually served — so the loop closes on
 * evidence rather than on the owner deciding the answer looked right.
 *
 * The questions are built from what this company has approved. A fixed list
 * would ask about a discount rule that a plumber's office does not have,
 * and the first thing the owner would learn is that the product is
 * describing somebody else.
 */
function TryIt({ company }: { company: string }) {
  const { objects } = useApp()
  const [copied, setCopied] = useState<string | null>(null)
  const [watching, setWatching] = useState(false)
  const [landed, setLanded] = useState<Usage | null>(null)
  const since = useRef<string | null>(null)

  const questions = useMemo(() => {
    const approved = objects.filter((o) => o.status === "approved")
    const first = (kind: string) => approved.find((o) => o.kind === kind)
    const rule = first("rule")
    const process = first("process")
    const term = first("term")

    return [
      rule ? `What does ${company} say about ${rule.title.toLowerCase()}?` : null,
      process ? `Walk me through ${process.title.toLowerCase()}, the way we actually do it.` : null,
      term ? `What does "${term.title}" mean at ${company}?` : null,
    ].filter((q) => q !== null)
  }, [objects, company])

  /**
   * Waits for the gateway to serve something it has not served before.
   *
   * Anchored to the newest read at the moment the question was copied, so
   * an old read from yesterday cannot be mistaken for this one.
   */
  useEffect(() => {
    if (!watching) return
    let cancelled = false

    const poll = async () => {
      const usage = await toolsApi.usage()
      if (cancelled) return
      const newest = usage[0]
      if (!newest) return
      if (since.current === null || newest.at > since.current) {
        setLanded(newest)
        setWatching(false)
      }
    }

    void poll()
    const timer = window.setInterval(poll, 2000)
    // Given up on after two minutes rather than spinning forever: somebody
    // who copied a question and went to lunch should come back to a screen
    // that is not still claiming to be waiting.
    const giveUp = window.setTimeout(() => setWatching(false), 120_000)
    return () => {
      cancelled = true
      window.clearInterval(timer)
      window.clearTimeout(giveUp)
    }
  }, [watching])

  const copy = async (question: string) => {
    const usage = await toolsApi.usage()
    since.current = usage[0]?.at ?? null
    try {
      await navigator.clipboard.writeText(question)
      setCopied(question)
      window.setTimeout(() => setCopied(null), 2000)
    } catch {
      /* clipboard blocked — the question is on screen to retype */
    }
    setLanded(null)
    setWatching(true)
  }

  if (questions.length === 0) {
    return (
      <div className="mt-7 rounded-xl border border-line bg-surface-2 p-4">
        <h2 className="text-[13px] font-medium text-ink">Try your company context</h2>
        <p className="mt-1 text-[12.5px] text-muted">
          Nothing is approved yet, so there is nothing for an AI tool to read. Approve something in
          Changes and a question to try will appear here.
        </p>
      </div>
    )
  }

  return (
    <div className="mt-7 rounded-xl border border-line bg-surface-2 p-4">
      <h2 className="text-[13px] font-medium text-ink">Try your company context</h2>
      <p className="mt-1 text-[12.5px] text-muted">
        Copy one into Claude or Codex. This panel is watching, and will say so when the answer came
        from {company}.
      </p>
      <div className="mt-3 space-y-1.5">
        {questions.map((question) => (
          <button
            key={question}
            type="button"
            onClick={() => void copy(question)}
            className="flex w-full items-center gap-2.5 rounded-lg border border-line bg-surface p-2.5 text-left text-[12.5px] text-muted transition-colors hover:border-accent hover:text-ink"
          >
            {copied === question ? (
              <Check className="size-3.5 shrink-0 text-confirmed" />
            ) : (
              <Copy className="size-3.5 shrink-0 text-faint" />
            )}
            <span className="flex-1">{question}</span>
          </button>
        ))}
      </div>

      {watching ? (
        <p className="mt-3 flex items-center gap-2 text-[12.5px] text-muted">
          <Loader2 className="size-3.5 animate-spin text-accent" />
          Waiting for a tool to read something…
        </p>
      ) : null}

      {landed ? (
        <div className="mt-3 rounded-lg border border-confirmed/40 bg-confirmed-soft p-3">
          <p className="flex items-center gap-2 text-[12.5px] font-medium text-confirmed">
            <Check className="size-3.5 shrink-0" />
            {landed.appLabel} read your company context
          </p>
          <ul className="mt-1.5 space-y-0.5">
            {landed.read.map((object) => (
              <li key={object.id} className="text-[12.5px] text-muted">
                {object.title}
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </div>
  )
}
