import { useCallback, useEffect, useState } from "react"
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
import { tools as toolsApi } from "@/lib/api"
import type { AiTool, ConnectPreview } from "@/lib/types"
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

/** The verb on the button, which is never just "Connect". */
function verb(tool: AiTool): string {
  if (tool.state === "needs-attention") return "Repair"
  if (tool.state === "connected") return tool.running ? "Open" : "Open"
  return tool.slug === "claude-desktop" ? "Install in Claude" : "Add Knowlith"
}

export function Connect() {
  const navigate = useNavigate()
  const { firstRun, setFirstRun, companyName } = useApp()

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
      setBusy(tool.slug)
      const result = await toolsApi.open(tool.slug)
      setNote(result?.message ?? null)
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

  return (
    <div className="mx-auto w-full max-w-[720px] px-5 py-10">
      <h1 className="text-[24px] font-semibold leading-tight tracking-[-0.022em] text-ink">
        Use your company context anywhere
      </h1>
      <p className="mt-2.5 max-w-[52ch] text-[14px] leading-relaxed text-muted">
        Connect the tools your team already uses. From then on they answer from what {companyName}{" "}
        approved, and they cite the document it came from.
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
                    <span className={`block text-[12.5px] ${state.tone}`}>
                      {state.label}
                      {tool.connected && tool.reads > 0 ? ` · ${tool.reads} context reads` : ""}
                    </span>
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
                      {verb(tool)}
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
              navigate("/home")
            }}
          >
            Done
            <ArrowRight />
          </Button>
          <button
            type="button"
            onClick={() => {
              setFirstRun(null)
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
 * The first question.
 *
 * Connecting something and being told it worked proves nothing. Asking one
 * question and getting the company's own answer back, with the document
 * named, is the moment the product becomes real — so it is on the screen
 * rather than left to the owner to think of.
 */
function TryIt({ company }: { company: string }) {
  const [copied, setCopied] = useState<string | null>(null)

  const questions = [
    "What discount can we approve for a regular customer?",
    "How do we put a quotation together?",
    `What does "bez PDV-a" mean at ${company}?`,
  ]

  const copy = async (question: string) => {
    try {
      await navigator.clipboard.writeText(question)
      setCopied(question)
      window.setTimeout(() => setCopied(null), 2000)
    } catch {
      /* clipboard blocked — the question is on screen to retype */
    }
  }

  return (
    <div className="mt-7 rounded-xl border border-line bg-surface-2 p-4">
      <h2 className="text-[13px] font-medium text-ink">Try your company context</h2>
      <p className="mt-1 text-[12.5px] text-muted">
        Ask one of these. The answer should name the document it came from — if it does not, the
        tool is not reading {company} yet.
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
    </div>
  )
}
