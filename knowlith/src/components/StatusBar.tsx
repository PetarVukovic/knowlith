import { Check, CircleDot, Cpu, Loader2, Pause } from "lucide-react"
import { Tooltip } from "@/components/ui/tooltip"
import { showingDemo } from "@/lib/api"
import { processorLabel } from "@/lib/processor"
import { formatCount, formatRelative } from "@/lib/utils"
import { useApp } from "@/state/AppState"

export function StatusBar() {
  const { sources, review, runs, mode, objects, live, work } = useApp()
  const active = sources.filter((s) => s.status === "active")
  const paused = sources.filter((s) => s.status === "paused")
  const lastSync = active.map((s) => s.lastSync).sort().at(-1)
  const processor = active[0]?.processor ?? "managed"
  const lastRun = runs[0]

  return (
    <footer className="flex h-7 shrink-0 items-center gap-4 overflow-x-auto border-t border-line bg-surface px-3 text-[11.5px] text-muted scroll-thin">
      <span className="flex shrink-0 items-center gap-1.5">
        <Check className={live ? "size-3 text-confirmed" : "size-3 text-faint"} />
        {/* Which data the screens are showing. Left unsaid, a demo lake and
            a real one look identical, and that is the one confusion this
            product cannot afford. */}
        {live
          ? lastSync
            ? `Synced ${formatRelative(lastSync)}`
            : "Connected"
          : showingDemo()
            ? "Example company — nothing here is real"
            : "Knowlith is not running — start it with `knowlith serve`"}
      </span>

      {/* With no folders there is nothing reading anything, and naming a
          processor here told an owner with an empty lake that their
          documents were being read in a cloud. */}
      {active.length > 0 ? (
        <span className="flex shrink-0 items-center gap-1.5">
          <Cpu className="size-3 text-faint" />
          {processorLabel(processor, mode)}
        </span>
      ) : null}

      {/* The onboarding screen says "you can close this window — it keeps
          going". This is the line that makes that checkable rather than
          reassuring: if nothing is here, nothing is happening. */}
      {work.queued + work.working > 0 ? (
        <span className="flex shrink-0 items-center gap-1.5 text-ink">
          <Loader2 className="size-3 animate-spin text-accent" />
          Still reviewing · {work.queued + work.working} left
        </span>
      ) : null}

      {paused.length > 0 ? (
        <span className="flex shrink-0 items-center gap-1.5 text-pending">
          <Pause className="size-3" />
          {paused.length} source paused
        </span>
      ) : null}

      {review.length > 0 ? (
        <span className="flex shrink-0 items-center gap-1.5 text-pending">
          <CircleDot className="size-3" />
          {review.length} waiting for you
        </span>
      ) : null}

      {mode === "engineer" ? (
        <span className="tabular flex shrink-0 items-center gap-1.5">
          {formatCount(objects.filter((o) => o.status === "approved").length)} objects in use
        </span>
      ) : null}

      {mode === "engineer" && lastRun ? (
        <Tooltip
          content={`${lastRun.rejectedUnsupported} candidates were dropped because no source span supported them.`}
        >
          <span className="shrink-0 font-mono text-[11px] text-faint tabular">
            {lastRun.id} · {lastRun.candidates} cand · {lastRun.accepted} kept ·{" "}
            {lastRun.rejectedUnsupported} unsupported
          </span>
        </Tooltip>
      ) : null}
    </footer>
  )
}
