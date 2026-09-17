import { Suspense, lazy, useCallback, useEffect, useMemo, useState } from "react"
import { useNavigate, useSearchParams } from "react-router-dom"
import { Loader2, Maximize2, Minimize2, RefreshCw } from "lucide-react"
import { BrainInspector } from "@/components/BrainInspector"
import { DocumentPreview } from "@/components/SourcePreview"
import { ResizeHandle, usePanelSize } from "@/components/Resizable"
import { Button } from "@/components/ui/button"
import { brain as brainApi } from "@/lib/api"
import { BRAIN_LEGEND, brainDocumentLakeId, brainKindMatches } from "@/lib/brainGraph"
import type { BrainNode, CompanyBrain } from "@/lib/types"
import { useApp } from "@/state/AppState"
import { cn } from "@/lib/utils"

const BrainGraph3D = lazy(() =>
  import("@/components/BrainGraph3D").then((m) => ({ default: m.BrainGraph3D })),
)

/**
 * The company brain: confirmed knowledge and the documents it quotes, laid
 * out as an interactive 3D knowledge graph. Click a
 * node to highlight its connections; ask about it in Claude Desktop, Codex
 * or a Terminal session the owner already connected.
 */
export function Brain() {
  const { companyName } = useApp()
  const navigate = useNavigate()
  const [params, setParams] = useSearchParams()
  const side = usePanelSize("brain-side-v4", 320, 260, 440)
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

  const countLine =
    data && counts.objects > 0
      ? `${counts.objects} confirmed ${counts.objects === 1 ? "item" : "items"} · ${counts.documents} ${counts.documents === 1 ? "document" : "documents"} quoted`
      : null

  return (
    <div
      data-fill-screen
      className={cn(
        "flex min-h-0 w-full bg-bg",
        fullscreen ? "fixed inset-0 z-50 flex-col" : "h-full flex-col md:flex-row",
      )}
    >
      <div className="relative flex min-h-0 min-w-0 flex-1 flex-col">
        <div
          className={cn(
            "relative min-h-0 flex-1 overflow-hidden bg-surface",
            fullscreen ? "border-0" : "md:border-r md:border-line",
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

          <div className="pointer-events-none absolute inset-x-0 top-0 flex items-start justify-between gap-3 p-3">
            <div className="pointer-events-none min-w-0">
              <h1 className="text-[14px] font-semibold text-ink">Company brain</h1>
              {countLine ? <p className="mt-0.5 truncate text-[12px] text-muted">{countLine}</p> : null}
            </div>
            <div className="pointer-events-auto flex shrink-0 items-center gap-1 rounded-md border border-line bg-surface p-0.5 shadow-k">
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

          {data && data.nodes.length > 0 ? (
            <ul className="pointer-events-auto absolute bottom-3 left-3 right-3 flex flex-wrap gap-1 md:right-auto">
              {BRAIN_LEGEND.map(([id, label, color]) => {
                const active = kindFilter === id
                return (
                  <li key={id}>
                    <button
                      type="button"
                      onClick={() => setKindFilter(id)}
                      className={cn(
                        "inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-[11.5px] shadow-k",
                        active
                          ? "border-accent bg-accent-soft text-ink"
                          : "border-line bg-surface text-muted hover:bg-surface-2 hover:text-ink",
                      )}
                    >
                      <span className="size-1.5 rounded-full" style={{ background: color }} aria-hidden />
                      {label}
                    </button>
                  </li>
                )
              })}
            </ul>
          ) : null}
        </div>
      </div>

      {!fullscreen ? (
        <ResizeHandle panel={side} edge="end" label="Resize detail panel" className="hidden md:block" />
      ) : null}

      <aside
        className={cn(
          "scroll-thin flex w-full shrink-0 flex-col overflow-y-auto border-t border-line bg-bg md:border-l md:border-t-0",
          fullscreen ? "max-h-[36vh]" : "max-h-[40vh] md:max-h-none",
        )}
        style={{ width: fullscreen ? undefined : `min(100%, ${side.width}px)` }}
      >
        {data ? (
          <BrainInspector
            companyName={companyName}
            kindFilter={kindFilter}
            selected={selected}
            edges={data.edges}
            nodes={data.nodes}
            assistants={data.assistants}
            onSelectId={setSelectedId}
            onClear={() => setSelectedId(null)}
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
