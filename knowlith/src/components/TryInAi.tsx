import { useEffect, useRef, useState } from "react"
import { useNavigate } from "react-router-dom"
import { Check, Loader2, Lock, Play } from "lucide-react"
import { AskAiPicker } from "@/components/AskAiPicker"
import { Button } from "@/components/ui/button"
import { tools as toolsApi } from "@/lib/api"
import { objectTryPrompt, tryButtonLabel } from "@/lib/askAi"
import type { ObjectKind, ObjectStatus } from "@/lib/types"

/**
 * Opens a connected AI with a question that names this piece of knowledge.
 *
 * A CLI assistant answers in the company chat on the Brain screen, with
 * this object in focus; a desktop app opens outside. Either way the panel
 * watches for a gateway read of *this* id — that is the proof, not
 * whatever the assistant printed.
 */
export function TryInAi({
  id,
  title,
  kind,
  status,
  company,
}: {
  id: string
  title: string
  kind: ObjectKind
  status: ObjectStatus
  company: string
}) {
  const navigate = useNavigate()
  const [hint, setHint] = useState<string | null>(null)
  const [watching, setWatching] = useState(false)
  const [readBy, setReadBy] = useState<string | null>(null)
  const [gaveUp, setGaveUp] = useState(false)
  const [pickerOpen, setPickerOpen] = useState(false)
  const since = useRef<string | null>(null)

  const question = objectTryPrompt(kind, title, company)
  const label = tryButtonLabel(kind)

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

  return (
    <>
      <div className="mt-6 rounded-lg border border-line bg-surface" data-tour="try-in-ai">
        <div className="flex flex-wrap items-center gap-3 px-3 py-2.5 sm:px-4">
          <Button variant="primary" onClick={() => void begin()}>
            <Play />
            {watching || readBy || gaveUp ? "Try again in your AI" : label}
          </Button>
          <p className="min-w-0 flex-1 text-[13px] text-muted">
            {hint ?? "Choose Claude, Codex or Cursor — CLIs answer in Knowlith's chat, desktop apps open outside."}
          </p>
          {readBy ? (
            <span className="flex shrink-0 items-center gap-1.5 text-[12.5px] text-confirmed">
              <Check className="size-3.5" />
              {readBy} read it.
            </span>
          ) : watching ? (
            <span className="flex shrink-0 items-center gap-1.5 text-[12.5px] text-faint">
              <Loader2 className="size-3.5 animate-spin" />
              Waiting…
            </span>
          ) : gaveUp ? (
            <span className="shrink-0 text-[12.5px] text-faint">
              Nothing read it in two minutes.
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
        onNeedsConnect={() => navigate("/connect")}
        onAskInside={(slug) =>
          navigate(`/brain?focus=${encodeURIComponent(id)}&agent=${encodeURIComponent(slug)}`)
        }
        onLaunched={(result) => {
          setHint(result.message)
          setWatching(true)
        }}
      />
    </>
  )
}
