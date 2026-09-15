import { FileSpreadsheet, FileText } from "lucide-react"
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog"
import type { Evidence, SourceBlock, SourceDocument } from "@/lib/types"
import { cn, formatDate } from "@/lib/utils"
import { useApp } from "@/state/AppState"

function matches(block: SourceBlock, evidence: Evidence) {
  if (block.text && block.text === evidence.quote) return true
  if (block.cells) {
    // Spreadsheet evidence is written as "cell | cell | cell"; the first cell
    // is enough to pin the row.
    const head = evidence.quote.split("|")[0]?.trim()
    if (head && block.cells.join(" ").includes(head)) return true
  }
  return block.locator === evidence.locator
}

/**
 * The original document, opened at the quote. Without this, "verified" is a
 * badge the user has to take on faith.
 */
export function SourcePreview({
  evidence,
  open,
  onOpenChange,
}: {
  evidence: Evidence
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const { documents } = useApp()
  const doc: SourceDocument | undefined = documents.find((d) => d.name === evidence.documentName)

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="w-[min(760px,calc(100vw-32px))] p-0">
        <div className="flex items-start gap-2.5 border-b border-line px-4 py-3 pr-12">
          {doc?.kind === "xlsx" ? (
            <FileSpreadsheet className="mt-0.5 size-4 shrink-0 text-faint" />
          ) : (
            <FileText className="mt-0.5 size-4 shrink-0 text-faint" />
          )}
          <div className="min-w-0">
            <DialogTitle className="truncate text-[13.5px] font-semibold text-ink">
              {evidence.documentName}
            </DialogTitle>
            <DialogDescription className="mt-0.5 truncate font-mono text-[11.5px] text-faint">
              {doc?.path ?? "path unavailable"}
            </DialogDescription>
          </div>
        </div>

        {doc ? (
          <>
            <div className="scroll-thin max-h-[58vh] overflow-y-auto px-4 py-4">
              {doc.columns ? <SheetView doc={doc} evidence={evidence} /> : <ProseView doc={doc} evidence={evidence} />}
            </div>
            <div className="flex flex-wrap items-center gap-x-4 gap-y-1 border-t border-line px-4 py-2.5 text-[11.5px] text-faint">
              <span>{evidence.locator}</span>
              <span className="tabular font-mono">
                bytes {evidence.startByte}–{evidence.endByte}
              </span>
              <span>last modified {formatDate(doc.modified)}</span>
              <span className="ml-auto">Excerpt. Knowlith opened this file read-only.</span>
            </div>
          </>
        ) : (
          <div className="px-4 py-8 text-center text-[12.5px] text-muted">
            This file is not available on this machine right now. The quote and its position are still recorded.
          </div>
        )}
      </DialogContent>
    </Dialog>
  )
}

function ProseView({ doc, evidence }: { doc: SourceDocument; evidence: Evidence }) {
  return (
    <div className="grid gap-2.5">
      {doc.blocks.map((block) => {
        const hit = matches(block, evidence)
        return (
          <p
            key={block.locator}
            className={cn(
              "rounded-sm px-2 py-1 text-[13px] leading-relaxed",
              block.heading ? "font-semibold text-ink" : "text-muted",
              hit && "bg-accent-soft text-ink ring-1 ring-accent/30",
            )}
          >
            {block.text}
          </p>
        )
      })}
    </div>
  )
}

function SheetView({ doc, evidence }: { doc: SourceDocument; evidence: Evidence }) {
  const columns = doc.columns ?? []
  return (
    <div className="overflow-x-auto">
      <table className="w-full border-collapse text-[12.5px]">
        <thead>
          <tr>
            {columns.map((c) => (
              <th key={c} className="border border-line bg-surface-3 px-2.5 py-1.5 text-left font-medium text-muted">
                {c}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {doc.blocks.map((block) => {
            const hit = matches(block, evidence)
            const cells = block.cells ?? []
            return (
              <tr key={block.locator} className={cn(hit && "bg-accent-soft")}>
                {cells.map((cell, ci) => (
                  <td
                    key={ci}
                    className={cn(
                      "border border-line px-2.5 py-1.5",
                      ci === cells.length - 1 && "tabular text-right",
                      hit ? "text-ink" : "text-muted",
                    )}
                  >
                    {cell}
                  </td>
                ))}
              </tr>
            )
          })}
        </tbody>
      </table>
    </div>
  )
}
