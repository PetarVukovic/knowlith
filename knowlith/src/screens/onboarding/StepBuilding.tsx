import { useEffect, useRef, useState } from "react"
import { Check, Loader2 } from "lucide-react"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"

/**
 * The first run, described in the owner's language.
 *
 * Every phase named here is something they can picture happening to their own
 * documents. Counting tokens, chunks or batches would be true and useless —
 * it tells them the machine is busy, not what it is doing to their files.
 */
const PHASES = [
  { key: "read", label: "Reading company files", detail: "Opening each document and keeping its structure." },
  { key: "terms", label: "Finding company terms", detail: "Words your company uses in its own way." },
  { key: "rules", label: "Identifying business rules", detail: "Limits, deadlines, and who approves what." },
  { key: "processes", label: "Reconstructing processes", detail: "The steps your team already follows." },
  { key: "conflicts", label: "Checking conflicts", detail: "Where two documents say different things." },
  { key: "review", label: "Preparing review", detail: "Nothing is live until you approve it." },
] as const

const PHASE_MS = 1400

export function StepBuilding({
  company,
  onDone,
  onLeave,
}: {
  company: string
  onDone: () => void
  onLeave: () => void
}) {
  const [phase, setPhase] = useState(0)
  const done = useRef(onDone)
  done.current = onDone

  useEffect(() => {
    const timer = window.setInterval(() => {
      setPhase((current) => {
        if (current >= PHASES.length - 1) {
          window.clearInterval(timer)
          window.setTimeout(() => done.current(), 700)
          return PHASES.length
        }
        return current + 1
      })
    }, PHASE_MS)
    return () => window.clearInterval(timer)
  }, [])

  const complete = Math.min(phase, PHASES.length)
  const percent = Math.round((complete / PHASES.length) * 100)

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
