import {
  AlertTriangle,
  BookMarked,
  Check,
  ChevronRight,
  FileText,
  Gavel,
  Hash,
  ListOrdered,
  Network,
  Radio,
  Sparkles,
} from "lucide-react"
import type { ComponentType } from "react"
import { Badge } from "@/components/ui/badge"
import { Tooltip } from "@/components/ui/tooltip"
import type { ContextObject, ObjectKind, ObjectStatus, Relation } from "@/lib/types"
import { cn, formatRelative } from "@/lib/utils"
import { useApp } from "@/state/AppState"

const KIND_META: Record<
  ObjectKind,
  { label: string; plural: string; meaning: string; Icon: ComponentType<{ className?: string }> }
> = {
  rule: {
    label: "Rule",
    plural: "Rules",
    meaning: "A decision your company already made — limits, deadlines, who approves what.",
    Icon: Gavel,
  },
  process: {
    label: "Process",
    plural: "Processes",
    meaning: "How work is done here, step by step, in the order your documents describe.",
    Icon: ListOrdered,
  },
  term: {
    label: "Business term",
    plural: "Business terms",
    meaning: "A word or product name your company uses in its own way.",
    Icon: BookMarked,
  },
  skill: {
    label: "AI skill",
    plural: "AI skills",
    meaning: "A task an AI assistant can carry out using only knowledge you confirmed.",
    Icon: Sparkles,
  },
  fact: {
    label: "Business info",
    plural: "Business info",
    meaning: "A figure or fact taken from your documents — not guessed.",
    Icon: FileText,
  },
}

export function kindMeta(kind: ObjectKind) {
  return KIND_META[kind]
}

/** One-line owner explanation — never the compiler taxonomy. */
export function kindMeaning(kind: ObjectKind): string {
  return KIND_META[kind].meaning
}

export function KindIcon({ kind, className }: { kind: ObjectKind; className?: string }) {
  const { Icon } = KIND_META[kind]
  return <Icon className={cn("size-3.5 text-faint", className)} />
}

export function KindBadge({ kind }: { kind: ObjectKind }) {
  return <Badge tone="outline">{KIND_META[kind].label}</Badge>
}

const STATUS_META: Record<ObjectStatus, { label: string; tone: "neutral" | "pending" | "conflict" | "confirmed" | "outline" }> = {
  draft: { label: "Needs confirmation", tone: "pending" },
  approved: { label: "In force", tone: "confirmed" },
  conflict: { label: "Documents disagree", tone: "conflict" },
  superseded: { label: "Replaced", tone: "neutral" },
  rejected: { label: "Rejected", tone: "neutral" },
}

export function StatusBadge({ status }: { status: ObjectStatus }) {
  const meta = STATUS_META[status]
  return <Badge tone={meta.tone}>{meta.label}</Badge>
}

export function StatusDot({ status, className }: { status: ObjectStatus; className?: string }) {
  const color = {
    draft: "bg-pending",
    approved: "bg-confirmed",
    conflict: "bg-conflict",
    superseded: "bg-line-strong",
    rejected: "bg-line-strong",
  }[status]
  return (
    <Tooltip content={STATUS_META[status].label}>
      <span className={cn("inline-block size-1.5 shrink-0 rounded-full", color, className)} />
    </Tooltip>
  )
}

/**
 * Confidence is shown as language to owners and as a number to engineers.
 * The number is calibrated from source agreement, not a model's self-report,
 * and saying "0.83" to a non-technical owner invites false precision.
 *
 * The thresholds and the words mirror `Confidence::phrase` in
 * `knowlith-core` exactly. They were not the same, and the result was that
 * every claim the compiler considered plainly stated arrived here labelled
 * "worth a closer look" — the one number in the product that has to mean the
 * same thing on both sides of the wire, meaning two different things.
 */
export function Confidence({ value, showBar = false }: { value: number; showBar?: boolean }) {
  const { mode } = useApp()
  const pct = Math.round(value * 100)
  const band = value >= 0.85 ? "high" : value >= 0.6 ? "mid" : value >= 0.35 ? "low" : "weak"
  const label = {
    high: "Clearly stated",
    mid: "Stated, in one place",
    low: "Implied, not written",
    weak: "Weakly supported",
  }[band]
  const tone = {
    high: "text-confirmed",
    mid: "text-muted",
    low: "text-pending",
    weak: "text-pending",
  }[band]
  const bar = {
    high: "bg-confirmed",
    mid: "bg-line-strong",
    low: "bg-pending",
    weak: "bg-pending",
  }[band]

  return (
    <span className="inline-flex items-center gap-1.5">
      {showBar ? (
        <span className="inline-block h-1 w-10 overflow-hidden rounded-full bg-surface-3">
          <span className={cn("block h-full rounded-full", bar)} style={{ width: `${pct}%` }} />
        </span>
      ) : null}
      <span className={cn("text-[12px]", tone)}>
        {mode === "engineer" ? `${(value).toFixed(2)} confidence` : label}
      </span>
    </span>
  )
}

const RELATION_LABEL: Record<Relation["type"], string> = {
  depends_on: "Uses",
  derived_from: "Derived from",
  supersedes: "Replaces",
  conflicts_with: "Disagrees with",
  used_by: "Used by",
}

export function RelationList({
  relations,
  onOpen,
  emptyLabel = "Nothing depends on this yet.",
}: {
  relations: Relation[]
  onOpen?: (id: string) => void
  emptyLabel?: string
}) {
  if (relations.length === 0) {
    return <div className="text-[12.5px] text-faint">{emptyLabel}</div>
  }
  return (
    <ul className="grid gap-1">
      {relations.map((r) => (
        <li key={`${r.type}-${r.targetId}`}>
          <button
            type="button"
            onClick={() => onOpen?.(r.targetId)}
            className="flex w-full items-baseline gap-2 rounded-sm px-1.5 py-1 text-left hover:bg-surface-3"
          >
            <span className="w-[74px] shrink-0 text-[11px] text-faint">{RELATION_LABEL[r.type]}</span>
            <span className="truncate text-[12.5px] text-ink">{r.targetTitle}</span>
            {/* A connection nothing can be checked against says so. Every
                other claim in this product carries a sentence from a
                document; a dependency between two rules is almost never
                written down anywhere, so this one is a proposal. */}
            {r.origin === "model" ? (
              <Tooltip content="Suggested by reading your rules together. No document states it outright.">
                <span className="ml-auto shrink-0 text-[10.5px] text-faint">suggested</span>
              </Tooltip>
            ) : null}
          </button>
        </li>
      ))}
    </ul>
  )
}

export function VerifiedMark({ verified }: { verified: boolean }) {
  const { mode } = useApp()
  return verified ? (
    <Tooltip content="The quoted text still exists at that exact position in the source file.">
      <span className="inline-flex items-center gap-1 text-[11px] text-confirmed">
        <Check className="size-3" />
        {mode === "engineer" ? "verified" : "exact quote"}
      </span>
    </Tooltip>
  ) : (
    <Tooltip content="The source file changed and this quote no longer matches. Recompile to fix.">
      <span className="inline-flex items-center gap-1 text-[11px] text-conflict">
        <AlertTriangle className="size-3" />
        {mode === "engineer" ? "stale" : "document changed"}
      </span>
    </Tooltip>
  )
}

export function MonoId({ id }: { id: string }) {
  return (
    <span className="inline-flex items-center gap-1 font-mono text-[11.5px] text-faint">
      <Hash className="size-3" />
      {id}
    </span>
  )
}

const SUBTYPE_LABEL: Record<NonNullable<ContextObject["subtype"]>, string> = {
  term: "Term",
  product: "Product",
  policy: "Policy",
  reference: "Reference",
  template: "Template",
}

/** A quiet label, never a navigation level. */
export function SubtypeLabel({ subtype }: { subtype?: ContextObject["subtype"] }) {
  if (!subtype) return null
  return <span className="text-[11.5px] text-faint">{SUBTYPE_LABEL[subtype]}</span>
}

/** `rule:sales.discount` → `rule`. The daemon guarantees the prefix. */
export function kindFromId(id: string): ObjectKind {
  const prefix = id.split(":")[0]
  if (prefix === "rule" || prefix === "process" || prefix === "skill" || prefix === "term" || prefix === "fact") {
    return prefix
  }
  return "fact"
}

function plural(n: number, one: string, many: string) {
  return `${n} ${n === 1 ? one : many}`
}

/**
 * What breaks if this changes. This is the difference between a knowledge base
 * and a compiled context, so it gets its own row rather than a tab nobody opens.
 */
export function ImpactStrip({
  relations,
  objectId,
  onOpen,
  onReads,
  className,
}: {
  relations: Relation[]
  objectId: string
  onOpen?: () => void
  /** Opens proof-of-use (Activity), when the owner taps the read line. */
  onReads?: () => void
  className?: string
}) {
  const { toolReads } = useApp()
  const used = relations.filter((r) => r.type === "used_by")
  const counts = new Map<ObjectKind, number>()
  for (const r of used) {
    const kind = kindFromId(r.targetId)
    counts.set(kind, (counts.get(kind) ?? 0) + 1)
  }
  const parts: string[] = []
  const processes = counts.get("process") ?? 0
  const skills = counts.get("skill") ?? 0
  const rules = counts.get("rule") ?? 0
  const knowledge = (counts.get("term") ?? 0) + (counts.get("fact") ?? 0)
  if (processes) parts.push(plural(processes, "process", "processes"))
  if (skills) parts.push(plural(skills, "skill", "skills"))
  if (rules) parts.push(plural(rules, "rule", "rules"))
  if (knowledge) parts.push(plural(knowledge, "knowledge item", "knowledge items"))

  const reads = toolReads[objectId] ?? []
  const lastRead = reads.map((r) => r.lastReadAt).sort().at(-1)

  if (parts.length === 0 && reads.length === 0) {
    return (
      <div className={cn("text-[12.5px] text-faint", className)}>
        Nothing depends on this yet, and no assistant has used it.
      </div>
    )
  }

  return (
    <div className={cn("flex flex-wrap items-center gap-x-2 gap-y-1.5", className)}>
      {parts.length > 0 ? (
        <button
          type="button"
          onClick={onOpen}
          className="inline-flex items-center gap-1.5 rounded-md border border-line bg-surface px-2 py-1 text-[12.5px] text-ink hover:border-line-strong"
        >
          <Network className="size-3.5 text-faint" />
          Also used in {parts.join(" · ")}
          <ChevronRight className="size-3 text-faint" />
        </button>
      ) : null}

      {reads.length > 0 ? (
        <Tooltip
          content={
            <span className="grid gap-0.5">
              {reads.map((r) => (
                <span key={r.tool}>
                  {r.tool} — last read {formatRelative(r.lastReadAt)}
                </span>
              ))}
            </span>
          }
        >
          <button
            type="button"
            onClick={onReads}
            className="inline-flex items-center gap-1.5 rounded-md border border-line bg-surface-2 px-2 py-1 text-[12.5px] text-muted hover:border-line-strong hover:text-ink"
          >
            <Radio className="size-3.5 text-faint" />
            Read by {plural(reads.length, "assistant", "assistants")}
            {lastRead ? <span className="text-faint">· {formatRelative(lastRead)}</span> : null}
          </button>
        </Tooltip>
      ) : null}
    </div>
  )
}

/**
 * The same four facts on every object: is it live, how well is it evidenced,
 * how many quotes back it, and who signed off.
 *
 * `compact` collapses the strip into one owner-facing sentence so Simple mode
 * does not read as a status dashboard.
 */
export function TrustStrip({
  status,
  confidence,
  evidenceCount,
  decidedBy,
  editedOnApproval,
  compact = false,
  className,
}: {
  status: ObjectStatus
  confidence: number
  evidenceCount: number
  decidedBy?: string
  editedOnApproval?: boolean
  compact?: boolean
  className?: string
}) {
  const { mode } = useApp()

  if (compact && mode === "simple") {
    const quotes = `${evidenceCount} source ${evidenceCount === 1 ? "quote" : "quotes"}`
    const who = decidedBy
      ? `approved by ${decidedBy}${editedOnApproval ? " (edited first)" : ""}`
      : "not approved yet"
    return (
      <p className={cn("flex flex-wrap items-center gap-x-2 gap-y-1 text-[13px] text-muted", className)}>
        <StatusBadge status={status} />
        <span>
          {quotes} · {who}
        </span>
      </p>
    )
  }

  return (
    <div className={cn("flex flex-wrap items-center gap-x-3 gap-y-1.5", className)}>
      <StatusBadge status={status} />
      <Confidence value={confidence} showBar />
      <span className="text-[12px] text-faint">
        {evidenceCount} source {evidenceCount === 1 ? "quote" : "quotes"}
      </span>
      {decidedBy ? (
        <span className="text-[12px] text-faint">
          approved by {decidedBy}
          {editedOnApproval ? " (edited first)" : ""}
        </span>
      ) : (
        <span className="text-[12px] text-pending">not approved yet</span>
      )}
    </div>
  )
}
