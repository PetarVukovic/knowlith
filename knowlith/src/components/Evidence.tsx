import { useState } from "react"
import { ExternalLink, FileText, Quote } from "lucide-react"
import { VerifiedMark } from "@/components/Domain"
import { SourcePreview } from "@/components/SourcePreview"
import type { Evidence } from "@/lib/types"
import { cn } from "@/lib/utils"
import { useApp } from "@/state/AppState"

/**
 * One source span. This component is the product's whole trust argument:
 * a claim is only as good as the sentence it points at, so the card opens the
 * original document at that sentence.
 */
export function EvidenceCard({
  evidence,
  compact = false,
  className,
}: {
  evidence: Evidence
  compact?: boolean
  className?: string
}) {
  const { mode } = useApp()
  const [open, setOpen] = useState(false)

  return (
    <>
      <button
        type="button"
        onClick={() => setOpen(true)}
        className={cn(
          "group w-full rounded-md border border-line bg-surface-2 p-2.5 text-left transition-colors hover:border-line-strong hover:bg-surface",
          className,
        )}
        aria-label={`Open ${evidence.documentName} at ${evidence.locator}`}
      >
        <span className="flex items-center justify-between gap-2">
          <span className="flex min-w-0 items-center gap-1.5">
            <FileText className="size-3.5 shrink-0 text-faint" />
            <span className="truncate text-[12.5px] font-medium text-ink">{evidence.documentName}</span>
            <span className="shrink-0 text-[12px] text-faint">· {evidence.locator}</span>
            <ExternalLink className="size-3 shrink-0 text-faint opacity-0 transition-opacity group-hover:opacity-100" />
          </span>
          <VerifiedMark verified={evidence.verified} />
        </span>

        {!compact ? (
          <span className="mt-2 flex gap-2 border-l-2 border-accent/40 pl-2.5">
            <Quote className="mt-0.5 size-3 shrink-0 text-faint" aria-hidden />
            <span className="text-[12.5px] leading-relaxed text-muted">{evidence.quote}</span>
          </span>
        ) : null}

        {mode === "engineer" ? (
          <span className="tabular mt-2 block font-mono text-[11px] text-faint">
            bytes {evidence.startByte}–{evidence.endByte}
            {evidence.page ? ` · page ${evidence.page}` : ""} · {evidence.documentId}
          </span>
        ) : null}
      </button>

      <SourcePreview evidence={evidence} open={open} onOpenChange={setOpen} />
    </>
  )
}

export function EvidenceList({ items, compact }: { items: Evidence[]; compact?: boolean }) {
  if (items.length === 0) {
    return <div className="text-[12.5px] text-faint">No source span attached. This object cannot be approved.</div>
  }
  return (
    <div className="grid gap-2">
      {items.map((e) => (
        <EvidenceCard key={e.id} evidence={e} compact={compact} />
      ))}
    </div>
  )
}
