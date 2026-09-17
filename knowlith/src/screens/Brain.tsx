import { Suspense, lazy, useCallback, useEffect, useMemo, useState } from "react"
import { useNavigate, useSearchParams } from "react-router-dom"
import { Loader2, Maximize2, Minimize2, Network, RefreshCw } from "lucide-react"
import { BrainInspector } from "@/components/BrainInspector"
import { DocumentPreview } from "@/components/SourcePreview"
import { ResizeHandle, usePanelSize } from "@/components/Resizable"
import { Button } from "@/components/ui/button"
import { brain as brainApi } from "@/lib/api"
import { brainDocumentLakeId, brainKindMatches } from "@/lib/brainGraph"
import type { BrainNode, CompanyBrain } from "@/lib/types"
import { useApp } from "@/state/AppState"
import { cn } from "@/lib/utils"

const BrainGraph3D = lazy(() =>
  import("@/components/BrainGraph3D").then((m) => ({ default: m.BrainGraph3D })),
)

/**
 * The company brain: confirmed knowledge and the documents it quotes, in
 * three dimensions. Click a node to highlight its connections; ask about it
 * in Claude Desktop, Codex or a Terminal session the owner already connected.
 */
export function Brain() {
  const { companyName } = useApp()
  const navigate = useNavigate()
  const [params, setParams] = useSearchParams()
  const side = usePanelSize("brain-side-v3", 300, 260, 420)
  const [data, setData] = useState<CompanyBrain | null>(null)
  const [selectedId, setSelectedId] = useState<string | null>(params.get("focus"))
  const [kindFilter, setKindFilter] = useState<string>("all")
  const [fullscreen, setFullscreen] = useState(false)
  const [resetSignal, setResetSignal] = useState(0)
  const [docPreviewId, setDocPreviewId] = useState<string | null>(null)

  const load = useCallback(async () => {
    setData(await brainApi.get())
  }, [])

  useEffect(() => {
    void load()
    const timer = window.setInterval(() => void load(), 12_000)
    return () => window.clearInterval(timer)
  }, [load])

  useEffect(() => {
    if (!selectedId || !data) return
    const node = data.nodes.find((n) => n.id === selectedId)
    if (node && !brainKindMatches(kindFilter, node.kind)) setSelectedId(null)
  }, [kindFilter, data, selectedId])

  useEffect(() => {
    if (!fullscreen) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setFullscreen(false)
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [fullscreen])

  const select = useCallback(
    (node: BrainNode | null) => {
      setSelectedId(node?.id ?? null)
      if (params.has("focus")) {
        params.delete("focus")
        setParams(params, { replace: true })
      }
    },
    [params, setParams],
  )

  const open = useCallback(
    (node: BrainNode) => {
      if (node.kind === "document") {
        setDocPreviewId(brainDocumentLakeId(node.id))
        return
      }
      navigate(
        node.kind === "skill"
          ? `/skills/${encodeURIComponent(node.id)}`
          : `/workspace/${encodeURIComponent(node.id)}`,
      )
    },
    [navigate],
  )

  const selected = useMemo(
    () => (selectedId ? data?.nodes.find((n) => n.id === selectedId) ?? null : null),
    [data, selectedId],
  )

  const counts = useMemo(() => {
    const c = { objects: 0, documents: 0 }
    for (const n of data?.nodes ?? []) {
      if (n.kind === "document") c.documents += 1
      else c.objects += 1
    }
    return c
  }, [data])

  return (
    <div
      data-fill-screen
      className={cn(
        "flex min-h-0 w-full bg-bg",
        fullscreen ? "fixed inset-0 z-50 flex-col" : "h-full flex-col md:flex-row",
      )}
    >
      <div className={cn("flex min-h-0 min-w-0 flex-1 flex-col", fullscreen ? "px-0 pt-0" : "px-3 pt-3")}>
        <div
          className={cn(
            "flex shrink-0 items-center gap-2",
            fullscreen ? "border-b border-line bg-surface px-3 py-1.5" : "px-1 pb-2",
          )}
        >
          <h1 className="flex min-w-0 items-center gap-1.5 text-[14px] font-semibold text-ink">
            <Network className="size-3.5 shrink-0 text-faint" />
            <span className="truncate">Company brain</span>
          </h1>
          {!fullscreen ? (
            <p className="hidden min-w-0 flex-1 truncate text-[12px] text-muted lg:block">
              {data && counts.objects > 0
                ? `${counts.objects} confirmed ${counts.objects === 1 ? "item" : "items"} · ${counts.documents} ${counts.documents === 1 ? "document" : "documents"} quoted · click to see connections, double-click to open`
                : "Click a node to see its connections · double-click to open"}
            </p>
          ) : (
            <span className="min-w-0 flex-1" />
          )}
          <div className="flex shrink-0 items-center gap-1">
            <Button variant="ghost" size="sm" onClick={() => setResetSignal((n) => n + 1)}>
              Fit
            </Button>
            <Button variant="ghost" size="sm" onClick={() => void load()}>
              <RefreshCw className="size-3.5" />
              <span className="sr-only sm:not-sr-only">Refresh</span>
            </Button>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setFullscreen((v) => !v)}
              aria-pressed={fullscreen}
            >
              {fullscreen ? (
                <>
                  <Minimize2 className="size-3.5" />
                  <span className="hidden sm:inline">Exit</span>
                </>
              ) : (
                <>
                  <Maximize2 className="size-3.5" />
                  <span className="hidden sm:inline">Full screen</span>
                </>
              )}
            </Button>
          </div>
        </div>

        <div
          className={cn(
            "relative min-h-0 flex-1 overflow-hidden bg-surface",
            fullscreen ? "border-0" : "rounded-lg border border-line",
          )}
        >
          {!data ? (
            <div className="grid h-full place-items-center text-faint">
              <Loader2 className="size-4 animate-spin" />
            </div>
          ) : data.nodes.length === 0 ? (
            <div className="grid h-full place-items-center p-8 text-center text-[13px] text-muted">
              No confirmed knowledge yet. Confirm items under For review and they appear here.
            </div>
          ) : (
            <Suspense
              fallback={
                <div className="grid h-full place-items-center text-faint">
                  <Loader2 className="size-4 animate-spin" />
                </div>
              }
            >
              <BrainGraph3D
                nodes={data.nodes}
                edges={data.edges}
                selectedId={selectedId}
                kindFilter={kindFilter}
                resetSignal={resetSignal}
                onSelect={select}
                onOpen={open}
              />
            </Suspense>
          )}
        </div>
      </div>

      <ResizeHandle panel={side} edge="end" label="Resize detail panel" className="hidden md:block" />

      <aside
        className="scroll-thin flex max-h-[40vh] w-full shrink-0 flex-col overflow-y-auto border-t border-line bg-bg md:max-h-none md:border-l md:border-t-0"
        style={{ width: side.width }}
      >
        {data ? (
          <BrainInspector
            companyName={companyName}
            kindFilter={kindFilter}
            onKindFilter={setKindFilter}
            selected={selected}
            edges={data.edges}
            nodes={data.nodes}
            assistants={data.assistants}
            onSelectId={setSelectedId}
            onOpen={open}
            onOpenDocument={setDocPreviewId}
          />
        ) : null}
      </aside>

      <DocumentPreview
        documentId={docPreviewId}
        open={docPreviewId !== null}
        onOpenChange={(open) => !open && setDocPreviewId(null)}
      />
    </div>
  )
}
