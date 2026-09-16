import { useState } from "react"
import {
  AlertTriangle,
  Cloud,
  FolderOpen,
  GitMerge,
  MoreHorizontal,
  Pause,
  Play,
  Plus,
  RefreshCw,
  Server,
  Terminal,
  Trash2,
  Wrench,
} from "lucide-react"
import { AddSource } from "@/components/AddSource"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Panel, PanelHeader } from "@/components/ui/surface"
import { processorLabel } from "@/lib/processor"
import type { Processor, Source } from "@/lib/types"
import { formatBytes, formatCount, formatRelative } from "@/lib/utils"
import { useApp } from "@/state/AppState"

/** Only the icon is fixed; the words come from `processorLabel`, which is mode-aware. */
const PROCESSOR_ICON: Record<Processor, typeof Terminal> = {
  codex: Terminal,
  "claude-code": Terminal,
  "cursor-agent": Wrench,
  managed: Cloud,
}

const STATUS_TONE = {
  active: "confirmed",
  paused: "pending",
  scanning: "info",
  error: "conflict",
} as const

export function Sources() {
  const { sources, setSourceStatus, removeSource, runs, mode } = useApp()
  const [pendingRemoval, setPendingRemoval] = useState<Source | null>(null)
  const [adding, setAdding] = useState(false)
  const [expanded, setExpanded] = useState<Record<string, boolean>>({})

  return (
    <div className="mx-auto w-full max-w-[880px] px-4 py-8">
      <AddSource open={adding} onClose={() => setAdding(false)} />

      <div className="flex flex-wrap items-baseline justify-between gap-3">
        <div>
          <h1 className="text-[20px] font-semibold tracking-[-0.015em] text-ink">Sources</h1>
          <p className="mt-1 max-w-[62ch] text-[13px] text-muted">
            Places Knowlith reads business information from. It never writes into them.
          </p>
        </div>
        <Button variant="primary" onClick={() => setAdding(true)}>
          <Plus />
          Add source
        </Button>
      </div>

      <div className="mt-6 grid gap-3">
        {sources.map((source) => {
          const ProcessorIcon = PROCESSOR_ICON[source.processor] ?? Terminal
          return (
            <Panel key={source.id}>
              <div className="flex flex-wrap items-start gap-3 p-4">
                <div className="mt-0.5 grid size-8 shrink-0 place-items-center rounded-md border border-line bg-surface-2">
                  {source.kind === "nas" ? (
                    <Server className="size-4 text-faint" />
                  ) : (
                    <FolderOpen className="size-4 text-faint" />
                  )}
                </div>

                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="text-[14px] font-medium text-ink">{source.name}</span>
                    <Badge tone={STATUS_TONE[source.status]}>
                      {source.status === "active"
                        ? "Reading"
                        : source.status === "paused"
                          ? "Paused"
                          : source.status === "scanning"
                            ? "Reading now"
                            : "Needs attention"}
                    </Badge>
                    {mode === "engineer" ? <Badge tone="outline">{source.access}</Badge> : null}
                  </div>
                  {mode === "engineer" ? (
                    <div className="mt-0.5 truncate font-mono text-[11.5px] text-faint">{source.path}</div>
                  ) : (
                    <div className="mt-0.5 text-[12.5px] text-muted">
                      {source.kind === "nas" ? "Network folder" : "Folder on this Mac"}
                      {" · "}
                      last read {formatRelative(source.lastAnalyzed)}
                    </div>
                  )}

                  <div className="mt-3 grid gap-1 text-[12.5px]">
                    {mode === "engineer" ? (
                      <span className="text-muted">Last analyzed {formatRelative(source.lastAnalyzed)}</span>
                    ) : null}
                    <span className={source.changesFound > 0 ? "text-ink" : "text-muted"}>
                      {source.changesFound > 0
                        ? `${source.changesFound} ${source.changesFound === 1 ? "update" : "updates"} waiting in Inbox`
                        : mode === "engineer"
                          ? "No context changes found"
                          : "Nothing new since last read"}
                    </span>
                    {source.conflictsFound > 0 ? (
                      <span className="flex items-center gap-1.5 text-conflict">
                        <GitMerge className="size-3.5" />
                        {source.conflictsFound} conflict detected
                      </span>
                    ) : null}
                  </div>

                  <div className="mt-3 flex flex-wrap items-center gap-x-4 gap-y-1 text-[12px] text-faint">
                    <span className="flex items-center gap-1.5">
                      <ProcessorIcon className="size-3.5" />
                      {processorLabel(source.processor, mode)}
                    </span>
                    <span className="tabular">
                      {formatCount(source.fileCount)} files · {formatBytes(source.bytes)}
                    </span>
                    <button
                      type="button"
                      onClick={() => setExpanded((e) => ({ ...e, [source.id]: !e[source.id] }))}
                      className="underline-offset-4 hover:text-muted hover:underline"
                    >
                      {expanded[source.id] ? "Hide file types" : "File types"}
                    </button>
                  </div>

                  {expanded[source.id] ? (
                    <div className="mt-2 flex flex-wrap gap-1.5">
                      {source.fileTypes.map((t) => (
                        <Badge key={t.ext} tone="neutral">
                          {t.ext} <span className="tabular text-faint">{formatCount(t.count)}</span>
                        </Badge>
                      ))}
                    </div>
                  ) : null}

                  {source.error ? (
                    <div className="mt-2 flex items-center gap-1.5 text-[12.5px] text-conflict">
                      <AlertTriangle className="size-3.5" />
                      {source.error}
                    </div>
                  ) : null}
                </div>

                <div className="flex shrink-0 items-center gap-1.5">
                  {source.status === "paused" ? (
                    <Button size="sm" onClick={() => setSourceStatus(source.id, "active")}>
                      <Play />
                      Resume
                    </Button>
                  ) : (
                    <Button size="sm" variant="ghost" onClick={() => setSourceStatus(source.id, "paused")}>
                      <Pause />
                      Pause
                    </Button>
                  )}
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => {
                      setSourceStatus(source.id, "scanning")
                      setTimeout(() => setSourceStatus(source.id, "active"), 1400)
                    }}
                  >
                    <RefreshCw className={source.status === "scanning" ? "animate-spin" : undefined} />
                    Read again
                  </Button>
                  <DropdownMenu>
                    <DropdownMenuTrigger asChild>
                      <Button size="icon-sm" variant="ghost" aria-label={`More options for ${source.name}`}>
                        <MoreHorizontal />
                      </Button>
                    </DropdownMenuTrigger>
                    <DropdownMenuContent>
                      <DropdownMenuItem>Change processor</DropdownMenuItem>
                      <DropdownMenuItem>Exclude subfolders</DropdownMenuItem>
                      <DropdownMenuSeparator />
                      <DropdownMenuItem
                        className="text-conflict data-[highlighted]:bg-conflict-soft"
                        onSelect={() => setPendingRemoval(source)}
                      >
                        <Trash2 />
                        Remove source
                      </DropdownMenuItem>
                    </DropdownMenuContent>
                  </DropdownMenu>
                </div>
              </div>
            </Panel>
          )
        })}
      </div>

      {mode === "engineer" ? (
        <Panel className="mt-8">
          <PanelHeader
            title="Compiler runs"
            description="Candidates the compiler produced, and how many survived the evidence check."
          />
          <div className="overflow-x-auto">
            <table className="w-full border-collapse text-[12.5px]">
              <thead>
                <tr className="border-b border-line text-left text-faint">
                  <th className="px-4 py-2 font-medium">Run</th>
                  <th className="px-4 py-2 font-medium">Processor</th>
                  <th className="px-4 py-2 text-right font-medium">Files</th>
                  <th className="px-4 py-2 text-right font-medium">Candidates</th>
                  <th className="px-4 py-2 text-right font-medium">Kept</th>
                  <th className="px-4 py-2 text-right font-medium">Unsupported</th>
                </tr>
              </thead>
              <tbody>
                {runs.map((run) => (
                  <tr key={run.id} className="border-b border-line last:border-0">
                    <td className="px-4 py-2 font-mono text-[11.5px] text-ink">{run.id}</td>
                    <td className="px-4 py-2 text-muted">{processorLabel(run.processor, "engineer")}</td>
                    <td className="tabular px-4 py-2 text-right text-muted">{formatCount(run.filesProcessed)}</td>
                    <td className="tabular px-4 py-2 text-right text-muted">{run.candidates}</td>
                    <td className="tabular px-4 py-2 text-right text-confirmed">{run.accepted}</td>
                    <td className="tabular px-4 py-2 text-right text-conflict">{run.rejectedUnsupported}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </Panel>
      ) : null}

      <Dialog open={pendingRemoval !== null} onOpenChange={(open) => !open && setPendingRemoval(null)}>
        <DialogContent>
          <DialogTitle className="text-[15px] font-semibold text-ink">
            Remove {pendingRemoval?.name}?
          </DialogTitle>
          <DialogDescription className="mt-1.5 text-[13px] text-muted">
            Knowlith stops reading this folder. Your files are untouched. Context already approved from it stays in
            use, but it will no longer update.
          </DialogDescription>
          <div className="mt-5 flex justify-end gap-2">
            <DialogClose asChild>
              <Button variant="ghost">Keep it</Button>
            </DialogClose>
            <Button
              variant="danger"
              onClick={() => {
                if (pendingRemoval) removeSource(pendingRemoval.id)
                setPendingRemoval(null)
              }}
            >
              Remove source
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  )
}
