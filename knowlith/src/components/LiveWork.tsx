import { useEffect, useRef, useState } from "react"
import { AlertTriangle, Check, Loader2, PauseCircle } from "lucide-react"
import { getWorkFeed } from "@/lib/api"
import type { Work } from "@/lib/types"
import { cn, formatRelative } from "@/lib/utils"

/**
 * What Knowlith is doing, while it is doing it.
 *
 * The workers have always produced a sentence for every job they finish —
 * "Cjenik 2026.xlsx: 14 claims · 2 not read". Until now that went to the
 * command line and nowhere else, so an owner who added a folder watched a
 * number count down with no account of what came out of it. This shows the
 * sentences.
 *
 * Two things it will not do. It gives no estimated time: a document takes
 * a second or a minute depending on its size and the engine, and the first
 * run has nothing to predict from, so any figure would be invented. And a
 * document that would not read stays on screen after the queue drains —
 * it is the one line here that asks the owner for something.
 */
export function LiveWork() {
  const [work, setWork] = useState<Work | null>(null)
  const timer = useRef<number | null>(null)

  useEffect(() => {
    let live = true

    const pull = async () => {
      const next = await getWorkFeed()
      if (!live) return
      setWork(next)
      // Once a second while there is something to watch, and rarely when
      // there is not. A panel that polls hard over an idle daemon is a
      // laptop fan spinning for nothing.
      const busy = next !== null && next.stage !== "idle"
      timer.current = window.setTimeout(pull, busy ? 1000 : 5000)
    }

    void pull()
    return () => {
      live = false
      if (timer.current !== null) window.clearTimeout(timer.current)
    }
  }, [])

  if (!work) return null

  const failures = work.lines.filter((line) => line.state === "failed")
  const busy = work.stage !== "idle"
  // An idle daemon with nothing to report is not worth a panel. One that
  // has just finished, or that could not read something, is.
  if (!busy && failures.length === 0 && work.lines.length === 0) return null

  const percent = work.total > 0 ? Math.round((work.done / work.total) * 100) : 100

  return (
    <section className="rounded-xl border border-line bg-surface">
      <header className="flex flex-wrap items-center gap-2.5 border-b border-line px-4 py-3">
        {busy ? (
          <Loader2 className="size-3.5 shrink-0 animate-spin text-accent" />
        ) : (
          <Check className="size-3.5 shrink-0 text-confirmed" />
        )}
        <span className="text-[13px] font-medium text-ink">
          {busy ? work.doing : "Up to date"}
        </span>
        {work.total > 0 ? (
          <span className="ml-auto font-mono text-[11.5px] tabular-nums text-faint">
            {work.done} of {work.total}
          </span>
        ) : null}
      </header>

      {work.total > 0 ? (
        <div className="h-[3px] w-full bg-surface-3" role="progressbar" aria-valuenow={percent}>
          <div
            className="h-full bg-accent transition-[width] duration-500"
            style={{ width: `${percent}%` }}
          />
        </div>
      ) : null}

      {work.held ? (
        <p className="flex items-center gap-2 border-b border-line bg-surface-2 px-4 py-2.5 text-[12.5px] text-muted">
          <PauseCircle className="size-3.5 shrink-0 text-pending" />
          <span>
            {work.held.count} {work.held.count === 1 ? "job is" : "jobs are"} waiting — {work.held.reason}.
          </span>
        </p>
      ) : null}

      <ol className="scroll-thin max-h-[280px] overflow-y-auto">
        {work.lines.map((line, index) => (
          <li
            key={`${line.at}-${index}`}
            className="flex items-baseline gap-2.5 border-b border-line/60 px-4 py-2 text-[12.5px] last:border-b-0"
          >
            {line.state === "failed" ? (
              <AlertTriangle className="size-3 shrink-0 translate-y-[2px] text-conflict" />
            ) : null}
            {/* Both halves shrink. A long file name held at its natural
                width pushes the row past the panel at phone size, and the
                page scrolls sideways. */}
            <span className={cn("min-w-0 flex-1 truncate", line.state === "failed" ? "text-conflict" : "text-ink")}>
              {line.subject}
            </span>
            <span className="min-w-0 flex-[2] truncate text-muted">{line.note}</span>
            <span className="shrink-0 font-mono text-[11px] tabular-nums text-faint">
              {formatRelative(line.at)}
            </span>
          </li>
        ))}
      </ol>
    </section>
  )
}
