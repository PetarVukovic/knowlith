import { useEffect, useRef, useState, type ReactNode } from "react"
import { useNavigate } from "react-router-dom"
import { Check, Loader2, Lock, Minimize2, Play, X } from "lucide-react"
import { AskAiPicker, type AskPickResult } from "@/components/AskAiPicker"
import { LiveTerminal } from "@/components/LiveTerminal"
import { ResizeHandle, usePanelSize } from "@/components/Resizable"
import { Button } from "@/components/ui/button"
import { tools as toolsApi } from "@/lib/api"
import { objectTryPrompt, tryButtonLabel } from "@/lib/askAi"
import type { ObjectKind, ObjectStatus } from "@/lib/types"
import { cn } from "@/lib/utils"

type LiveSession = AskPickResult & { prompt: string }

/**
 * Opens a connected AI with a question that names this piece of knowledge.
 *
 * CLI assistants open a live PTY beside the claim (same pattern as Company
 * brain). Desktop apps still open outside. The panel also watches for a
 * gateway read of *this* id — that is the proof, not the terminal scrollback.
 */
export function TryInAi({
  id,
  title,
  kind,
  status,
  company,
  context,
  embedded = true,
}: {
  id: string
  title: string
  kind: ObjectKind
  status: ObjectStatus
  company: string
  /** Shown beside the live terminal so the claim stays readable while asking. */
  context?: ReactNode
  /** Prefer in-app PTY for CLI hosts (Workspace / skills / brain). */
  embedded?: boolean
}) {
  const navigate = useNavigate()
  const side = usePanelSize("try-ai-side", 380, 280, 720)
  const [hint, setHint] = useState<string | null>(null)
  const [watching, setWatching] = useState(false)
  const [readBy, setReadBy] = useState<string | null>(null)
  const [gaveUp, setGaveUp] = useState(false)
  const [pickerOpen, setPickerOpen] = useState(false)
  const [session, setSession] = useState<LiveSession | null>(null)
  const since = useRef<string | null>(null)

  const question = objectTryPrompt(kind, title, company)
  const label = tryButtonLabel(kind)
  const live = Boolean(session?.embedded)

  useEffect(() => {
    if (!watching) return
    let cancelled = false
    const poll = async () => {
      const usage = await toolsApi.usage()
      if (cancelled) return
      const landed = usage.find(
        (row) =>
          (since.current === null || row.at > since.current) &&
          row.read.some((object) => object.id === id),
      )
      if (landed) {
        setReadBy(landed.appLabel)
        setWatching(false)
      }
    }
    void poll()
    const timer = window.setInterval(poll, 2000)
    const giveUp = window.setTimeout(() => {
      setWatching(false)
      setGaveUp(true)
    }, 120_000)
    return () => {
      cancelled = true
      window.clearInterval(timer)
      window.clearTimeout(giveUp)
    }
  }, [watching, id])

  useEffect(() => {
    if (!live) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setSession(null)
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [live])

  if (status !== "approved") {
    return (
      <div className="mt-6 flex flex-wrap items-center gap-2.5 rounded-lg border border-line bg-surface-2 px-4 py-3 text-[13px]">
        <Lock className="size-3.5 shrink-0 text-pending" />
        <span className="text-ink">No AI tool can use this yet.</span>
        <span className="text-muted">
          Confirm it first — then it becomes part of what assistants may read.
        </span>
      </div>
    )
  }

  const begin = async () => {
    const usage = await toolsApi.usage()
    since.current = usage[0]?.at ?? null
    setReadBy(null)
    setGaveUp(false)
    setHint(null)
    setPickerOpen(true)
  }

  const closeLive = () => setSession(null)

  return (
    <>
      <div className="mt-6 rounded-lg border border-line bg-surface" data-tour="try-in-ai">
        <div className="flex flex-wrap items-center gap-3 px-3 py-2.5 sm:px-4">
          <Button variant="primary" onClick={() => void begin()}>
            <Play />
            {watching || readBy || gaveUp || session ? "Try again in your AI" : label}
          </Button>
          <p className="min-w-0 flex-1 text-[13px] text-muted">
            {hint
              ? hint
              : embedded
                ? "Choose Claude, Codex or Cursor — CLI opens live beside this claim."
                : "Choose Claude, Codex or Cursor — opens a real Terminal or desktop app."}
          </p>
          {readBy ? (
            <span className="flex shrink-0 items-center gap-1.5 text-[12.5px] text-confirmed">
              <Check className="size-3.5" />
              {readBy} opened it.
            </span>
          ) : watching ? (
            <span className="flex shrink-0 items-center gap-1.5 text-[12.5px] text-faint">
              <Loader2 className="size-3.5 animate-spin" />
              Waiting…
            </span>
          ) : gaveUp ? (
            <span className="shrink-0 text-[12.5px] text-faint">
              Nothing opened it in two minutes.
            </span>
          ) : null}
        </div>
        {hint || watching ? (
          <p className="border-t border-line px-3 py-2 text-[12.5px] italic text-muted sm:px-4">
            “{question}”
          </p>
        ) : null}
      </div>

      <AskAiPicker
        open={pickerOpen}
        onOpenChange={setPickerOpen}
        prompt={question}
        about={title}
        embedded={embedded}
        onNeedsConnect={() => navigate("/connect")}
        onLaunched={(result) => {
          setHint(result.message)
          setWatching(true)
          if (result.embedded) {
            setSession({ ...result, prompt: question })
          } else {
            setSession(null)
          }
        }}
      />

      {live && session ? (
        <div
          data-fill-screen
          className="fixed inset-0 z-50 flex bg-bg"
          role="dialog"
          aria-label={`Live ${session.label} session`}
        >
          <div className="flex min-w-0 flex-1 flex-col">
            <div className="flex shrink-0 items-center gap-2 border-b border-line bg-surface px-3 py-1.5">
              <div className="min-w-0 flex-1">
                <div className="truncate text-[13px] font-semibold text-ink">{title}</div>
                <p className="truncate text-[11.5px] text-muted">{session.message}</p>
              </div>
              {readBy ? (
                <span className="hidden shrink-0 items-center gap-1 text-[12px] text-confirmed sm:flex">
                  <Check className="size-3.5" />
                  {readBy} read it
                </span>
              ) : watching ? (
                <span className="hidden shrink-0 items-center gap-1 text-[12px] text-faint sm:flex">
                  <Loader2 className="size-3 animate-spin" />
                  Waiting for a read…
                </span>
              ) : null}
              <Button variant="ghost" size="sm" onClick={closeLive}>
                <Minimize2 className="size-3.5" />
                <span className="hidden sm:inline">Back</span>
              </Button>
              <button
                type="button"
                className="text-faint hover:text-ink"
                onClick={closeLive}
                aria-label="Close live session"
              >
                <X className="size-3.5" />
              </button>
            </div>
            <div className="scroll-thin min-h-0 flex-1 overflow-y-auto px-4 py-4 sm:px-6">
              {context ? (
                <div className="mx-auto w-full max-w-[640px]">{context}</div>
              ) : (
                <p className="text-[13px] italic text-muted">“{question}”</p>
              )}
            </div>
          </div>

          <ResizeHandle
            panel={side}
            edge="end"
            label="Resize live terminal"
            className="hidden md:block"
          />

          <aside
            className={cn("flex shrink-0 flex-col border-l border-line bg-[#0c0c0c]")}
            style={{ width: side.width }}
          >
            <LiveTerminal
              key={`${session.slug}:${session.prompt}`}
              app={session.slug}
              prompt={session.prompt}
              label={`${session.label} · ${title}`}
              onClose={closeLive}
            />
          </aside>
        </div>
      ) : null}
    </>
  )
}
