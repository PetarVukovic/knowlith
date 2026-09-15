import { useNavigate } from "react-router-dom"
import { ArrowRight, CheckCircle2, ChevronRight, GitMerge, PauseCircle, RefreshCw } from "lucide-react"
import { KindIcon, kindMeta } from "@/components/Domain"
import { Button } from "@/components/ui/button"
import { Panel } from "@/components/ui/surface"
import type { ObjectKind } from "@/lib/types"
import { cn, formatRelative } from "@/lib/utils"
import { useApp } from "@/state/AppState"

const COUNTED: ObjectKind[] = ["rule", "process", "skill", "term"]

/**
 * The home screen answers three questions before the user clicks anything:
 * what does Knowlith know, what changed, and is anything waiting on me.
 * Opening straight into the context tree answers none of them.
 */
export function Home() {
  const { companyName, objects, skills, review, sources, activity, mode, discovery } = useApp()
  const navigate = useNavigate()

  const approved = objects.filter((o) => o.status === "approved")
  const draft = objects.filter((o) => o.status !== "approved")
  const counts: Record<ObjectKind, number> = {
    rule: approved.filter((o) => o.kind === "rule").length,
    process: approved.filter((o) => o.kind === "process").length,
    skill: skills.filter((s) => s.status === "approved").length,
    term: approved.filter((o) => o.kind === "term" || o.kind === "fact").length,
    fact: 0,
  }
  // The tiles count what is in use, the tree lists everything including
  // drafts. Without this line the two numbers look like a bug at a glance.
  const waiting: Record<ObjectKind, number> = {
    rule: draft.filter((o) => o.kind === "rule").length,
    process: draft.filter((o) => o.kind === "process").length,
    skill: skills.filter((s) => s.status !== "approved").length,
    term: draft.filter((o) => o.kind === "term" || o.kind === "fact").length,
    fact: 0,
  }

  const conflicts = review.filter((r) => r.conflict).length
  const paused = sources.filter((s) => s.status === "paused").length
  const attention = [
    review.length > 0
      ? {
          id: "changes",
          Icon: ChevronRight,
          tone: "pending" as const,
          label: `${review.length} proposed ${review.length === 1 ? "change" : "changes"}`,
          detail: "Nothing reaches your AI tools until you decide.",
          action: () => navigate("/review"),
          cta: "Review",
        }
      : null,
    conflicts > 0
      ? {
          id: "conflicts",
          Icon: GitMerge,
          tone: "conflict" as const,
          label: `${conflicts} conflicting ${conflicts === 1 ? "document" : "documents"}`,
          detail: "Two files state different things about the same subject.",
          action: () => navigate("/review"),
          cta: "Resolve",
        }
      : null,
    paused > 0
      ? {
          id: "paused",
          Icon: PauseCircle,
          tone: "pending" as const,
          label: `${paused} source paused`,
          detail: "Changes in that folder are not being picked up.",
          action: () => navigate("/sources"),
          cta: "Open sources",
        }
      : null,
  ].filter((x) => x !== null)

  const healthy = attention.length === 0

  return (
    <div className="mx-auto w-full max-w-[880px] px-4 py-8">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h1 className="text-[22px] font-semibold tracking-[-0.02em] text-ink">{companyName}</h1>
          <p
            className={cn(
              "mt-1 flex items-center gap-1.5 text-[13px]",
              healthy ? "text-confirmed" : "text-muted",
            )}
          >
            {healthy ? <CheckCircle2 className="size-3.5" /> : null}
            {healthy
              ? "Company context is healthy"
              : `Company context is in use, with ${attention.length} ${attention.length === 1 ? "thing" : "things"} waiting on you`}
          </p>
        </div>
        <Button variant="default" onClick={() => navigate("/sources")}>
          <RefreshCw />
          Read sources again
        </Button>
      </div>

      <div className="mt-7 grid grid-cols-2 gap-px overflow-hidden rounded-lg border border-line bg-line sm:grid-cols-4">
        {COUNTED.map((kind) => (
          <button
            key={kind}
            type="button"
            onClick={() => {
              if (kind === "skill") {
                if (skills[0]) navigate(`/skills/${encodeURIComponent(skills[0].id)}`)
                return
              }
              const first = approved.find((o) =>
                kind === "term" ? o.kind === "term" || o.kind === "fact" : o.kind === kind,
              )
              if (first) navigate(`/workspace/${encodeURIComponent(first.id)}`)
            }}
            className="bg-surface px-4 py-3.5 text-left transition-colors hover:bg-surface-2"
          >
            <div className="tabular text-[26px] font-semibold leading-none text-ink">{counts[kind]}</div>
            <div className="mt-1.5 flex flex-wrap items-center gap-x-1.5 text-[12px] text-muted">
              <KindIcon kind={kind} />
              {kindMeta(kind).plural}
              {waiting[kind] > 0 ? (
                <span className="text-pending">+{waiting[kind]} waiting</span>
              ) : null}
            </div>
          </button>
        ))}
      </div>

      <h2 className="mt-9 text-[15px] font-semibold text-ink">Needs your attention</h2>
      {healthy ? (
        <Panel className="mt-3 p-4 text-[13px] text-muted">
          Nothing is waiting. New findings appear here the next time Knowlith reads your folders.
        </Panel>
      ) : (
        <div className="mt-3 grid gap-2">
          {attention.map((item) => (
            <Panel key={item.id} className="flex flex-wrap items-center gap-3 p-3.5">
              <span
                className={cn(
                  "grid size-7 shrink-0 place-items-center rounded-md",
                  item.tone === "conflict" ? "bg-conflict-soft text-conflict" : "bg-pending-soft text-pending",
                )}
              >
                <item.Icon className="size-3.5" />
              </span>
              <span className="min-w-0 flex-1">
                <span className="block text-[13.5px] font-medium text-ink">{item.label}</span>
                <span className="block text-[12.5px] text-muted">{item.detail}</span>
              </span>
              <Button size="sm" variant={item.tone === "conflict" ? "primary" : "default"} onClick={item.action}>
                {item.cta}
                <ArrowRight />
              </Button>
            </Panel>
          ))}
        </div>
      )}

      <h2 className="mt-9 text-[15px] font-semibold text-ink">Recent</h2>
      <ul className="mt-3 grid gap-px overflow-hidden rounded-lg border border-line bg-line">
        {activity.map((event) => (
          <li key={event.id} className="flex flex-wrap items-baseline gap-x-3 gap-y-1 bg-surface px-3.5 py-2.5">
            <span
              className={cn(
                "mt-1.5 size-1.5 shrink-0 rounded-full",
                event.tone === "conflict" ? "bg-conflict" : event.tone === "pending" ? "bg-pending" : "bg-line-strong",
              )}
            />
            <span className="min-w-0 flex-1">
              <span className="block text-[13px] text-ink">{event.title}</span>
              <span className="block text-[12.5px] text-muted">{event.detail}</span>
            </span>
            <span className="shrink-0 text-[12px] text-faint">
              {event.source} · {formatRelative(event.at)}
            </span>
          </li>
        ))}
      </ul>

      {mode === "engineer" && discovery ? (
        <p className="mt-6 font-mono text-[11.5px] text-faint">
          last full compile: {discovery.filesRead} files · {discovery.spansExtracted} spans ·{" "}
          {discovery.durationSeconds}s
        </p>
      ) : null}
    </div>
  )
}
