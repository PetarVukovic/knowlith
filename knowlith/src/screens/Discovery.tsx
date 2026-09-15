import { Link, useNavigate } from "react-router-dom"
import { ArrowRight, GitMerge } from "lucide-react"
import { Confidence, KindIcon, kindMeta } from "@/components/Domain"
import { EvidenceCard } from "@/components/Evidence"
import { Button } from "@/components/ui/button"
import { Panel } from "@/components/ui/surface"
import { formatCount } from "@/lib/utils"
import { useApp } from "@/state/AppState"

export function Discovery() {
  const { discovery, review, mode } = useApp()
  const navigate = useNavigate()

  if (!discovery) return null

  const tiles = [
    { label: "Rules", value: discovery.rules, tone: "text-ink" },
    { label: "Processes", value: discovery.processes, tone: "text-ink" },
    { label: "Company terms", value: discovery.terms, tone: "text-ink" },
    { label: "Skills", value: discovery.skills, tone: "text-ink" },
    { label: "Conflicts", value: discovery.conflicts, tone: "text-conflict" },
  ]

  return (
    <div className="mx-auto w-full max-w-[880px] px-4 py-10 sm:py-14">
      <h1 className="text-[24px] font-semibold leading-tight tracking-[-0.02em] text-ink">
        We started understanding your company
      </h1>
      <p className="mt-1.5 max-w-[60ch] text-[13.5px] text-muted">
        Knowlith read {formatCount(discovery.filesRead)} files in{" "}
        {Math.round(discovery.durationSeconds / 60)} minutes. Nothing below is in use yet — it becomes part of your
        company context only after you approve it.
      </p>

      <div className="mt-8 grid grid-cols-2 gap-px overflow-hidden rounded-lg border border-line bg-line sm:grid-cols-5">
        {tiles.map((t) => (
          <div key={t.label} className="bg-surface px-4 py-3.5">
            <div className={`tabular text-[26px] font-semibold leading-none ${t.tone}`}>{t.value}</div>
            <div className="mt-1.5 text-[12px] text-muted">{t.label}</div>
          </div>
        ))}
      </div>

      {mode === "engineer" ? (
        <div className="mt-2 font-mono text-[11.5px] text-faint tabular">
          {formatCount(discovery.spansExtracted)} evidence spans extracted · {formatCount(discovery.filesRead)} files ·{" "}
          {discovery.durationSeconds}s wall clock
        </div>
      ) : null}

      <div className="mt-10 flex items-baseline justify-between gap-4">
        <h2 className="text-[15px] font-semibold text-ink">Waiting for you</h2>
        <span className="text-[12.5px] text-muted">{review.length} items</span>
      </div>

      <div className="mt-3 grid gap-2">
        {review.slice(0, 5).map((item) => (
          <Panel key={item.id} className="p-3.5">
            <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
              <KindIcon kind={item.kind} />
              <span className="text-[13.5px] font-medium text-ink">{item.title}</span>
              <span className="text-[12px] text-faint">{kindMeta(item.kind).label}</span>
              {item.conflict ? (
                <span className="inline-flex items-center gap-1 rounded-sm bg-conflict-soft px-1.5 py-px text-[11px] font-medium text-conflict">
                  <GitMerge className="size-3" />
                  Two documents disagree
                </span>
              ) : null}
              <span className="ml-auto">
                <Confidence value={item.confidence} showBar />
              </span>
            </div>
            <div className="mt-2.5">
              <EvidenceCard evidence={item.evidence[0]} />
            </div>
          </Panel>
        ))}
      </div>

      {review.length > 5 ? (
        <div className="mt-2 text-[12.5px] text-muted">and {review.length - 5} more</div>
      ) : null}

      <div className="mt-8 flex flex-wrap items-center gap-3">
        <Button size="lg" variant="primary" onClick={() => navigate("/review")}>
          Review discoveries
          <ArrowRight />
        </Button>
        <Link to="/home" className="text-[12.5px] text-muted underline-offset-4 hover:text-ink hover:underline">
          Look at the workspace first
        </Link>
      </div>
    </div>
  )
}
