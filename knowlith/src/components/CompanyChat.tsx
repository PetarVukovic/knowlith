import { useCallback, useEffect, useRef, useState } from "react"
import { ArrowUp, Loader2, MessageSquare, Square, X } from "lucide-react"
import { Button } from "@/components/ui/button"
import { terminalSocketUrl, tools as toolsApi } from "@/lib/api"
import type { AiTool, BrainNode } from "@/lib/types"
import { cn } from "@/lib/utils"

/** CLI agents that run on the owner's machine via PTY — never Knowlith→API. */
const CLI_SLUGS = new Set(["claude-code", "codex", "cursor"])

export type ChatLit = {
  nodeIds: string[]
  /** Edges to glow: from-to pairs among lit nodes. */
  edgeKeys: string[]
}

type ChatMessage =
  | {
      id: string
      role: "user"
      text: string
      about?: string
    }
  | {
      id: string
      role: "assistant"
      text: string
      agent: string
      status: "connecting" | "streaming" | "done" | "error"
      error?: string
    }

type LiveRun = {
  slug: string
  label: string
  prompt: string
  since: string | null
  assistantId: string
}

/**
 * Company chat UI. The owner's CLI runs on a hidden PTY; this panel is only
 * bubbles — never an xterm. Ban-safe: Knowlith never calls Anthropic/OpenAI HTTP.
 */
export function CompanyChat({
  companyName,
  focusNode,
  preferredAgent = null,
  onLit,
  onNeedsConnect,
}: {
  companyName: string
  focusNode: BrainNode | null
  /** The assistant another screen already chose (`?agent=` from Try in AI). */
  preferredAgent?: string | null
  onLit: (lit: ChatLit) => void
  onNeedsConnect: () => void
}) {
  const [tools, setTools] = useState<AiTool[] | null>(null)
  const [agent, setAgent] = useState<string | null>(null)
  const [draft, setDraft] = useState("")
  const [messages, setMessages] = useState<ChatMessage[]>([])
  const [live, setLive] = useState<LiveRun | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const bottomRef = useRef<HTMLDivElement>(null)
  const sinceRef = useRef<string | null>(null)
  const wsRef = useRef<WebSocket | null>(null)
  const rawBuf = useRef("")

  useEffect(() => {
    let cancelled = false
    void toolsApi.list().then((list) => {
      if (cancelled) return
      setTools(list)
      const connected = list.filter((t) => t.connected && CLI_SLUGS.has(t.slug))
      setAgent((prev) => {
        if (prev && connected.some((t) => t.slug === prev)) return prev
        if (preferredAgent && connected.some((t) => t.slug === preferredAgent)) return preferredAgent
        const prefer = ["claude-code", "codex", "cursor"] as const
        for (const slug of prefer) {
          if (connected.some((t) => t.slug === slug)) return slug
        }
        return connected[0]?.slug ?? null
      })
    })
    return () => {
      cancelled = true
    }
  }, [preferredAgent])

  useEffect(() => {
    if (!focusNode || live) return
    setDraft((prev) => {
      if (prev.trim()) return prev
      return `Explain “${focusNode.title}” the way ${companyName} actually uses it, and cite what you read.`
    })
  }, [focusNode?.id, companyName, focusNode, live])

  // While a session is live, light up nodes the gateway recorded as read.
  useEffect(() => {
    if (!live) {
      onLit({ nodeIds: [], edgeKeys: [] })
      return
    }
    let cancelled = false
    const poll = async () => {
      const usage = await toolsApi.usage()
      if (cancelled) return
      const fresh = usage.filter((row) => {
        if (sinceRef.current && row.at <= sinceRef.current) return false
        if (live.since && row.at <= live.since) return false
        return true
      })
      const ids = new Set<string>()
      for (const row of fresh) {
        for (const obj of row.skipped) ids.add(obj.id)
        for (const obj of row.read) ids.add(obj.id)
      }
      onLit({ nodeIds: [...ids], edgeKeys: [] })
    }
    void poll()
    const timer = window.setInterval(poll, 2000)
    return () => {
      cancelled = true
      window.clearInterval(timer)
    }
  }, [live, onLit])

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth", block: "end" })
  }, [messages, live])

  const patchAssistant = useCallback(
    (id: string, patch: Partial<Extract<ChatMessage, { role: "assistant" }>>) => {
      setMessages((prev) =>
        prev.map((m) => (m.id === id && m.role === "assistant" ? { ...m, ...patch } : m)),
      )
    },
    [],
  )

  const stopLive = useCallback(() => {
    const ws = wsRef.current
    wsRef.current = null
    if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) {
      ws.close()
    }
    setLive((cur) => {
      if (!cur) return null
      const id = cur.assistantId
      setMessages((prev) =>
        prev.map((m) => {
          if (m.id !== id || m.role !== "assistant") return m
          if (m.status === "streaming") {
            return { ...m, status: "done" }
          }
          if (m.status === "connecting") {
            return {
              ...m,
              status: "error",
              error: "Stopped before the assistant started.",
            }
          }
          return m
        }),
      )
      return null
    })
  }, [])

  // Tear down the socket if the component unmounts mid-run.
  useEffect(() => {
    return () => {
      const ws = wsRef.current
      wsRef.current = null
      if (ws) ws.close()
    }
  }, [])

  const startPty = useCallback(
    (run: LiveRun) => {
      rawBuf.current = ""
      const ws = new WebSocket(terminalSocketUrl())
      ws.binaryType = "arraybuffer"
      wsRef.current = ws

      ws.onopen = () => {
        ws.send(JSON.stringify({ type: "start", app: run.slug, prompt: run.prompt }))
      }

      ws.onmessage = (ev) => {
        if (typeof ev.data === "string") {
          try {
            const msg = JSON.parse(ev.data) as {
              type: string
              message?: string
            }
            if (msg.type === "ready") {
              patchAssistant(run.assistantId, { status: "streaming" })
              return
            }
            if (msg.type === "error") {
              patchAssistant(run.assistantId, {
                status: "error",
                error: msg.message ?? "Could not start the assistant.",
              })
              setLive(null)
              wsRef.current = null
              return
            }
            if (msg.type === "exit") {
              setMessages((prev) =>
                prev.map((m) => {
                  if (m.id !== run.assistantId || m.role !== "assistant") return m
                  const text = m.text.trim()
                  return {
                    ...m,
                    status: "done" as const,
                    text:
                      text ||
                      "Finished. If nothing appeared above, the assistant may have answered only in its own window.",
                  }
                }),
              )
              setLive(null)
              wsRef.current = null
            }
          } catch {
            // Non-JSON text frames are rare; treat as output.
            appendPtyText(run.assistantId, ev.data)
          }
          return
        }
        const bytes = new Uint8Array(ev.data as ArrayBuffer)
        appendPtyText(run.assistantId, new TextDecoder().decode(bytes))
      }

      ws.onerror = () => {
        setMessages((prev) => {
          const cur = prev.find((m) => m.id === run.assistantId)
          if (cur?.role === "assistant" && cur.status !== "connecting") return prev
          return prev.map((m) =>
            m.id === run.assistantId && m.role === "assistant"
              ? {
                  ...m,
                  status: "error" as const,
                  error: "Could not reach Knowlith’s local agent. Is the daemon running?",
                }
              : m,
          )
        })
        setLive(null)
        wsRef.current = null
      }

      ws.onclose = () => {
        if (wsRef.current !== ws) return
        wsRef.current = null
        setLive((cur) => (cur?.assistantId === run.assistantId ? null : cur))
        setMessages((prev) =>
          prev.map((m) =>
            m.id === run.assistantId && m.role === "assistant" && m.status === "streaming"
              ? { ...m, status: "done" as const }
              : m,
          ),
        )
      }

      function appendPtyText(assistantId: string, chunk: string) {
        rawBuf.current += chunk
        const cleaned = ptyToChatText(rawBuf.current)
        if (/you've hit your usage limit/i.test(rawBuf.current)) {
          patchAssistant(assistantId, {
            status: "error",
            error:
              "This assistant hit its usage limit. Try another connected CLI, or wait until the limit resets.",
          })
          return
        }
        if (!cleaned) return
        patchAssistant(assistantId, { text: cleaned, status: "streaming" })
      }
    },
    [patchAssistant],
  )

  const cliAgents = (tools ?? []).filter((t) => CLI_SLUGS.has(t.slug))
  const connectedCli = cliAgents.filter((t) => t.connected)
  const answering = Boolean(live)

  const send = useCallback(async () => {
    const text = draft.trim()
    if (!text || busy || live) return
    if (!agent) {
      onNeedsConnect()
      return
    }
    const tool = (tools ?? []).find((t) => t.slug === agent)
    if (!tool?.connected) {
      onNeedsConnect()
      return
    }

    setBusy(true)
    setError(null)
    const usage = await toolsApi.usage()
    const since = usage[0]?.at ?? null
    sinceRef.current = since

    const about = focusNode?.title
    const prompt = buildPrompt(text, companyName, about)

    setBusy(false)

    const assistantId = `a-${Date.now()}`
    setMessages((prev) => [
      ...prev,
      { id: `u-${Date.now()}`, role: "user", text, about },
      {
        id: assistantId,
        role: "assistant",
        text: "",
        agent: shortLabel(tool.slug),
        status: "connecting",
      },
    ])
    setDraft("")
    const run: LiveRun = {
      slug: tool.slug,
      label: tool.label,
      prompt,
      since,
      assistantId,
    }
    setLive(run)
    startPty(run)
  }, [draft, busy, live, agent, tools, companyName, focusNode, onNeedsConnect, startPty])

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="shrink-0 border-b border-line px-3 py-2.5">
        <div className="flex items-center gap-1.5 text-[11px] font-medium uppercase tracking-wide text-faint">
          <MessageSquare className="size-3" />
          Ask your company
        </div>
        <p className="mt-0.5 text-[12px] text-muted">
          Answers from your Mac via the assistant you pick — never a Knowlith cloud call.
        </p>

        <div className="mt-2.5 flex flex-wrap gap-1.5">
          {cliAgents.length === 0 && tools !== null ? (
            <button
              type="button"
              onClick={onNeedsConnect}
              className="rounded-md border border-line px-2 py-1 text-[12px] text-pending hover:bg-surface-2"
            >
              Connect an assistant
            </button>
          ) : (
            cliAgents.map((tool) => {
              const on = agent === tool.slug
              const ready = tool.connected
              return (
                <button
                  key={tool.slug}
                  type="button"
                  disabled={!ready || answering}
                  onClick={() => setAgent(tool.slug)}
                  className={cn(
                    "rounded-full border px-2.5 py-1 text-[12px]",
                    on
                      ? "border-accent bg-accent-soft text-accent"
                      : ready
                        ? "border-line text-ink hover:bg-surface-2"
                        : "border-line text-faint opacity-60",
                  )}
                  title={ready ? tool.label : "Not connected"}
                >
                  {shortLabel(tool.slug)}
                </button>
              )
            })
          )}
        </div>
      </div>

      {focusNode ? (
        <div className="shrink-0 border-b border-line bg-surface-2 px-3 py-2 text-[12px]">
          <span className="text-faint">About </span>
          <span className="font-medium text-ink">{focusNode.title}</span>
        </div>
      ) : null}

      <div className="min-h-0 flex-1 overflow-y-auto px-3 py-3">
        {messages.length === 0 ? (
          <div className="rounded-lg border border-dashed border-line bg-surface-2/60 px-3 py-4">
            <p className="text-[13px] font-medium text-ink">
              Ask anything {companyName} has confirmed
            </p>
            <p className="mt-1 text-[12.5px] leading-relaxed text-muted">
              The assistant reads approved knowledge through Knowlith. As it
              reads, those spots light up on the brain map.
            </p>
            <ul className="mt-3 space-y-1.5 text-[12.5px] text-muted">
              <li>· What do we charge for …?</li>
              <li>· How do we handle …?</li>
              <li>· Click a node, then ask about it</li>
            </ul>
          </div>
        ) : null}

        <ul className="space-y-3">
          {messages.map((msg) =>
            msg.role === "user" ? (
              <li key={msg.id} className="flex justify-end">
                <div className="max-w-[92%] rounded-2xl rounded-br-md bg-accent px-3 py-2 text-[13px] leading-relaxed text-white">
                  {msg.about ? (
                    <div className="mb-1 text-[10.5px] uppercase tracking-wide text-white/70">
                      About {msg.about}
                    </div>
                  ) : null}
                  {msg.text}
                </div>
              </li>
            ) : (
              <li key={msg.id} className="flex justify-start">
                <div className="max-w-[96%] rounded-2xl rounded-bl-md border border-line bg-surface px-3 py-2.5 text-[13px] leading-relaxed text-ink shadow-k">
                  <div className="mb-1.5 flex items-center gap-2 text-[10.5px] uppercase tracking-wide text-faint">
                    <span>{msg.agent}</span>
                    {msg.status === "connecting" || msg.status === "streaming" ? (
                      <span className="inline-flex items-center gap-1 text-pending normal-case tracking-normal">
                        <Loader2 className="size-3 animate-spin" />
                        {msg.status === "connecting" ? "Starting…" : "Reading…"}
                      </span>
                    ) : null}
                  </div>
                  {msg.status === "error" ? (
                    <p className="text-conflict">{msg.error ?? "Something went wrong."}</p>
                  ) : msg.text ? (
                    <p className="whitespace-pre-wrap break-words">{msg.text}</p>
                  ) : msg.status === "connecting" || msg.status === "streaming" ? (
                    <p className="text-muted">Working on your Mac…</p>
                  ) : (
                    <p className="text-muted">No reply captured.</p>
                  )}
                </div>
              </li>
            ),
          )}
        </ul>

        {error ? <p className="mt-2 text-[12.5px] text-conflict">{error}</p> : null}
        <div ref={bottomRef} />
      </div>

      <div className="shrink-0 border-t border-line p-3">
        {answering ? (
          <div className="mb-2 flex items-center justify-between gap-2 rounded-lg border border-line bg-surface-2 px-2.5 py-1.5">
            <span className="text-[12px] text-muted">
              {live?.label ?? "Assistant"} is answering on this Mac
            </span>
            <Button size="sm" variant="ghost" onClick={stopLive} aria-label="Stop">
              <Square className="size-3 fill-current" />
              Stop
            </Button>
          </div>
        ) : null}
        <div className="flex items-end gap-2 rounded-xl border border-line bg-surface px-2.5 py-2 focus-within:border-line-strong">
          <textarea
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault()
                void send()
              }
            }}
            rows={2}
            disabled={answering || busy}
            placeholder={
              connectedCli.length === 0
                ? "Connect Claude Code, Codex or Cursor Agent first…"
                : answering
                  ? "Waiting for this answer…"
                  : `Ask ${companyName}…`
            }
            className="min-h-[44px] max-h-28 flex-1 resize-none bg-transparent text-[13px] text-ink outline-none placeholder:text-faint"
          />
          {answering ? (
            <Button size="icon-sm" variant="ghost" onClick={stopLive} aria-label="Stop">
              <X />
            </Button>
          ) : (
            <Button
              size="icon-sm"
              variant="primary"
              disabled={!draft.trim() || busy || !agent}
              onClick={() => void send()}
              aria-label="Send"
            >
              {busy ? <Loader2 className="animate-spin" /> : <ArrowUp />}
            </Button>
          )}
        </div>
        <p className="mt-1.5 text-[11px] text-faint">
          One question at a time · your assistant subscription · Enter to send
        </p>
      </div>
    </div>
  )
}

function shortLabel(slug: string): string {
  switch (slug) {
    case "claude-code":
      return "Claude Code"
    case "codex":
      return "Codex"
    case "cursor":
      return "Cursor Agent"
    default:
      return slug
  }
}

function buildPrompt(question: string, company: string, about?: string): string {
  const focus = about
    ? `The owner is looking at “${about}” on the company map. Prefer knowledge that touches that.\n\n`
    : ""
  return (
    `${focus}` +
    `You are answering from ${company}'s own approved knowledge via the Knowlith MCP server.\n` +
    `Start with get_relevant_context for this question. Read what you need with get_context / ` +
    `lookup_value / get_process / get_skill. Finish with check_coverage. Cite document names.\n` +
    `Do not invent prices, deadlines or policies. If something is open, say it is open.\n` +
    `Write a clear chat reply — no terminal chrome, no ASCII boxes.\n\n` +
    `Question:\n${question}`
  )
}

/** Turn raw PTY bytes into readable chat text (ANSI / OSC / spinner noise out). */
function ptyToChatText(raw: string): string {
  let s = raw
    .replace(/\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)/g, "")
    .replace(/\x1b\[[0-9;?]*[ -/]*[@-~]/g, "")
    .replace(/\x1b[()][0-9A-Za-z]/g, "")
    .replace(/\x1b./g, "")
    .replace(/\r\n/g, "\n")
    .replace(/\r/g, "\n")
    .replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f]/g, "")

  const lines = s.split("\n").filter((line) => {
    const t = line.trim()
    if (!t) return true
    if (/^[⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏◐◓◑◒|/\\-]+$/.test(t)) return false
    if (/^[-=_]{4,}$/.test(t)) return false
    if (/^[╭╮╯╰│─┌┐└┘├┤┬┴┼▀▄█░▒▓╔╗╚╝║═╠╣╦╩╬▌▐>]+$/.test(t)) return false
    // Codex / Cursor interactive chrome that still leaks in print mode.
    if (/^>_?\s*OpenAI Codex/i.test(t)) return false
    if (/^model:\s*/i.test(t)) return false
    if (/^directory:\s*/i.test(t)) return false
    if (/^::\s*Working/i.test(t)) return false
    if (/ctrl\+c to stop/i.test(t)) return false
    if (/shift\+tab to cycle/i.test(t)) return false
    if (/^Ask\b.*cycle/i.test(t)) return false
    if (/^Auto\b$/i.test(t)) return false
    if (/^~\s*$/.test(t)) return false
    if (/MCP startup incomplete/i.test(t)) return false
    if (/Skill descriptions were shortened/i.test(t)) return false
    if (/You've hit your usage limit/i.test(t)) return false
    if (/^>\s*Ask Codex/i.test(t)) return false
    if (/Context \d+%\s*left/i.test(t)) return false
    if (/^[0-9;mu>]{4,}$/.test(t)) return false
    return true
  })
  s = lines.join("\n").replace(/\n{3,}/g, "\n\n").trim()
  return s
}

/** Edges between lit nodes — computed in the brain from the live graph. */
export function edgeKeysAmong(nodeIds: string[], edges: { from: string; to: string }[]): string[] {
  const set = new Set(nodeIds)
  const keys: string[] = []
  for (const e of edges) {
    if (set.has(e.from) && set.has(e.to)) {
      keys.push(`${e.from}|${e.to}`)
    }
  }
  return keys
}
