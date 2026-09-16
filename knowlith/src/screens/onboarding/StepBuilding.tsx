import { useEffect, useRef, useState } from "react"
import { AlertTriangle, Check, Loader2 } from "lucide-react"
import { Button } from "@/components/ui/button"
import { api } from "@/lib/api"
import type { Health } from "@/lib/types"
import { cn, formatCount } from "@/lib/utils"
import { useApp } from "@/state/AppState"

/**
 * The first run, described in the owner's language.
 *
 * Every phase named here is something they can picture happening to their own
 * documents. Counting tokens, chunks or batches would be true and useless —
 * it tells them the machine is busy, not what it is doing to their files.
 *
 * What moves this screen is the daemon's own job queue, not a timer. That
 * matters more than it looks: a progress bar that finishes on schedule while
 * the work is still running is the single fastest way to lose somebody's
 * trust in everything else on the screen.
 */
const PHASES = [
  { key: "read", label: "Reading company files", detail: "Opening each document and keeping its structure." },
  { key: "terms", label: "Finding company terms", detail: "Words your company uses in its own way." },
  { key: "rules", label: "Identifying business rules", detail: "Limits, deadlines, and who approves what." },
  { key: "processes", label: "Reconstructing processes", detail: "The steps your team already follows." },
  { key: "conflicts", label: "Checking conflicts", detail: "Where two documents say different things." },
  { key: "review", label: "Preparing review", detail: "Nothing is live until you approve it." },
] as const

const POLL_MS = 2000

export function StepBuilding({
  company,
  onDone,
  onLeave,
}: {
  company: string
  onDone: () => void
  onLeave: () => void
}) {
  const [work, setWork] = useState<Health | null>(null)
  /** The most the queue ever held, which is what the bar is a fraction of. */
  const [peak, setPeak] = useState(0)
  const [read, setRead] = useState(0)
  const { refresh } = useApp()
  // Held in refs so the poll below is started once and never restarted by a
  // parent that re-renders with a fresh callback.
  const done = useRef(onDone)
  const pull = useRef(refresh)
  useEffect(() => {
    done.current = onDone
    pull.current = refresh
  }, [onDone, refresh])

  useEffect(() => {
    let cancelled = false
    let seenWork = false

    const poll = async () => {
      const next = await api.getWork()
      if (cancelled) return
      setWork(next)
      setRead(next.objects)
      const outstanding = next.queued + next.working
      setPeak((current) => Math.max(current, outstanding))
      // Work counts as seen either because the queue held something or
      // because something came out of it. With recorded replies a small
      // company compiles in under a second, and a screen that only watches
      // the queue would wait here forever for work that is already done.
      if (outstanding > 0 || next.objects > 0) seenWork = true
      // Finished means the queue is empty after there was something to do.
      // Without that guard this screen would advance during the second
      // between the folder being added and the first job being picked up.
      // Everything is fetched before the next screen appears, so it opens
      // with the company's own numbers rather than with zeros that fill in a
      // few seconds later.
      if (seenWork && outstanding === 0) {
        cancelled = true
        await pull.current()
        done.current()
      }
    }

    void poll()
    const timer = window.setInterval(poll, POLL_MS)
    return () => {
      cancelled = true
      window.clearInterval(timer)
    }
  }, [])

  const outstanding = work ? work.queued + work.working : 0
  const percent = peak > 0 ? Math.round(((peak - outstanding) / peak) * 100) : 0

  // Which phase to light up. The queue cannot say which of six stages a
  // document is in, so this maps honestly onto how much of the queue is
  // gone rather than pretending to a precision the daemon does not report.
  const phase = Math.min(PHASES.length - 1, Math.floor((percent / 100) * PHASES.length))
  const stalled = work !== null && peak === 0

  return (
    <div>
      <h1 className="text-[26px] font-semibold leading-tight tracking-[-0.022em] text-ink">
        Reading {company}
      </h1>
      <p className="mt-2.5 text-[14px] leading-relaxed text-muted">
        This takes a few minutes the first time. You can close this window — it keeps going.
      </p>

      <div className="mt-7 flex items-center gap-3">
        <span className="h-1.5 flex-1 overflow-hidden rounded-full bg-surface-3">
          <span
            className="block h-full rounded-full bg-accent transition-[width] duration-700 ease-out"
            style={{ width: `${percent}%` }}
          />
        </span>
        <span className="tabular w-10 shrink-0 text-right text-[12.5px] text-muted">{percent}%</span>
      </div>

      <p className="tabular mt-2 text-[12px] text-faint">
        {outstanding > 0
          ? `${formatCount(outstanding)} left · ${formatCount(read)} found so far`
          : `${formatCount(read)} found so far`}
      </p>

      {stalled ? (
        <p className="mt-5 flex items-start gap-2 text-[13px] text-pending">
          <AlertTriangle className="mt-[2px] size-4 shrink-0" />
          Nothing is queued. If this does not move, the background worker is not running —
          the status bar at the bottom says which.
        </p>
      ) : null}

      <ul className="mt-8 grid gap-3.5">
        {PHASES.map((item, index) => {
          const state = index < phase ? "done" : index === phase ? "running" : "waiting"
          return (
            <li key={item.key} className="flex gap-3">
              <span className="mt-0.5 shrink-0">
                {state === "done" ? (
                  <Check className="size-[18px] text-confirmed" />
                ) : state === "running" ? (
                  <Loader2 className="size-[18px] animate-spin text-accent" />
                ) : (
                  <span className="block size-[18px] rounded-full border border-line-strong" />
                )}
              </span>
              <span className="min-w-0">
                <span
                  className={cn(
                    "block text-[14px] transition-colors",
                    state === "waiting" ? "text-faint" : "font-medium text-ink",
                  )}
                >
                  {item.label}
                </span>
                {state !== "waiting" ? (
                  <span className="mt-0.5 block text-[12.5px] text-muted">{item.detail}</span>
                ) : null}
              </span>
            </li>
          )
        })}
      </ul>

      <Button variant="ghost" size="sm" onClick={onLeave} className="mt-9">
        Let it run in the background
      </Button>
    </div>
  )
}
