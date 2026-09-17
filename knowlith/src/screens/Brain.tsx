import { Suspense, lazy, useCallback, useEffect, useMemo, useState } from "react"
import { useNavigate, useSearchParams } from "react-router-dom"
import { ExternalLink, Loader2, Maximize2, Minimize2, Network, RefreshCw } from "lucide-react"
import { CompanyChat, type ChatLit } from "@/components/CompanyChat"
import { ResizeHandle, usePanelSize } from "@/components/Resizable"
import { Button } from "@/components/ui/button"
import { brain as brainApi } from "@/lib/api"
import type { BrainNode, CompanyBrain } from "@/lib/types"
import { useApp } from "@/state/AppState"
import { cn } from "@/lib/utils"

// three.js is the heaviest thing in the bundle and only this screen wants it.
const BrainGraph3D = lazy(() =>
  import("@/components/BrainGraph3D").then((m) => ({ default: m.BrainGraph3D })),
)

const LEGEND = [
  ["all", "All", "#94a3b8"],
  ["rule", "Rules", "#3b82f6"],
  ["process", "Processes", "#10b981"],
  ["skill", "Skills", "#f59e0b"],
  ["fact", "Terms", "#8b5cf6"],
  ["document", "Documents", "#8b979c"],
] as const

/**
 * The company brain: every confirmed rule, process, term and skill, the
 * documents they quote, and the arrows between them, in three dimensions.
 * The chat beside it runs the owner's own CLI; what that assistant reads
 * lights up on the map while it answers.
 */
export function Brain() {
  const { companyName } = useApp()
  const navigate = useNavigate()
  const [params, setParams] = useSearchParams()
  const side = usePanelSize("brain-side-v2", 340, 260, 640)
  const [data, setData] = useState<CompanyBrain | null>(null)
  const [selectedId, setSelectedId] = useState<string | null>(params.get("focus"))
  const [kindFilter, setKindFilter] = useState<string>("all")
  const [fullscreen, setFullscreen] = useState(false)
  const [litIds, setLitIds] = useState<string[]>([])
  const [resetSignal, setResetSignal] = useState(0)
  const preferredAgent = params.get("agent")

  const onChatLit = useCallback((lit: ChatLit) => setLitIds(lit.nodeIds), [])

  const load = useCallback(async () => {
    setData(await brainApi.get())
  }, [])

  useEffect(() => {
    void load()
    const timer = window.setInterval(() => void load(), 12_000)
    return () => window.clearInterval(timer)
  }, [load])

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
      // `?focus=` came from another screen; once the owner picks something
      // else the address should not keep pointing at the old thing.
      if (params.has("focus")) {
        params.delete("focus")
        setParams(params, { replace: true })
      }
    },
    [params, setParams],
  )

  const open = useCallback(
    (node: BrainNode) => {
      if (node.kind === "document") return
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
  const around = useMemo(() => {
    if (!selected || !data) return []
    const title = (id: string) => data.nodes.find((n) => n.id === id)?.title ?? id
    return data.edges
      .filter((e) => e.from === selected.id || e.to === selected.id)
      .map((e) =>
        e.from === selected.id
          ? { key: `${e.from}|${e.to}|${e.type}`, text: `${e.label} ${title(e.to)}`, id: e.to }
          : { key: `${e.from}|${e.to}|${e.type}`, text: `${title(e.from)} ${e.label} this`, id: e.from },
      )
  }, [selected, data])

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
      className={cn("flex min-h-0 w-full bg-bg", fullscreen ? "fixed inset-0 z-50" : "h-full")}
    >
      <div className={cn("flex min-w-0 flex-1 flex-col", fullscreen ? "px-0 pt-0" : "px-3 pt-3")}>
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
                ? `${counts.objects} confirmed ${counts.objects === 1 ? "item" : "items"} · ${counts.documents} ${counts.documents === 1 ? "document" : "documents"} quoted · drag to turn, scroll to zoom, click to pick, double-click to open`
                : "Drag to turn, scroll to zoom, click to pick, double-click to open"}
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
                litIds={litIds}
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

      <ResizeHandle panel={side} edge="end" label="Resize chat panel" className="hidden md:block" />

      <aside
        className="flex shrink-0 flex-col border-l border-line bg-bg"
        style={{ width: side.width }}
      >
        {data ? (
          <div className="shrink-0 border-b border-line px-3 py-2">
            <ul className="flex flex-wrap gap-1">
              {LEGEND.map(([id, label, color]) => {
                const active = kindFilter === id
                return (
                  <li key={id}>
                    <button
                      type="button"
                      onClick={() => setKindFilter(id)}
                      className={cn(
                        "inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[11.5px]",
                        active
                          ? "border-accent bg-accent-soft text-ink"
                          : "border-line text-muted hover:bg-surface-2",
                      )}
                    >
                      <span className="size-1.5 rounded-full" style={{ background: color }} aria-hidden />
                      {label}
                    </button>
                  </li>
                )
              })}
            </ul>
            {litIds.length > 0 ? (
              <p className="mt-2 text-[11.5px] text-pending">
                {litIds.length} {litIds.length === 1 ? "item" : "items"} read by the assistant so far
              </p>
            ) : null}
          </div>
        ) : null}

        {selected ? (
          <div className="shrink-0 border-b border-line px-3 py-2.5">
            <div className="flex items-start gap-2">
              <span
                className="mt-1 size-2 shrink-0 rounded-full"
                style={{ background: LEGEND.find(([id]) => id === selected.kind || (id === "fact" && selected.kind === "term"))?.[2] ?? "#94a3b8" }}
                aria-hidden
              />
              <div className="min-w-0 flex-1">
                <div className="truncate text-[13px] font-medium text-ink">{selected.title}</div>
                <div className="text-[11.5px] text-faint">
                  {selected.kind === "document"
                    ? "Document — quoted by what it is joined to"
                    : selected.kind === "term"
                      ? "Business term"
                      : selected.kind[0].toUpperCase() + selected.kind.slice(1)}
                </div>
              </div>
              {selected.kind !== "document" ? (
                <Button size="sm" variant="subtle" onClick={() => open(selected)}>
                  <ExternalLink className="size-3.5" />
                  Open
                </Button>
              ) : null}
            </div>
            {around.length > 0 ? (
              <ul className="mt-2 grid gap-1">
                {around.slice(0, 8).map((a) => (
                  <li key={a.key}>
                    <button
                      type="button"
                      className="w-full truncate text-left text-[12px] text-muted hover:text-ink"
                      onClick={() => setSelectedId(a.id)}
                    >
                      {a.text}
                    </button>
                  </li>
                ))}
                {around.length > 8 ? (
                  <li className="text-[11.5px] text-faint">and {around.length - 8} more</li>
                ) : null}
              </ul>
            ) : (
              <p className="mt-2 text-[12px] text-faint">Joined to nothing else yet.</p>
            )}
          </div>
        ) : null}

        <div className="min-h-0 flex-1">
          <CompanyChat
            companyName={companyName}
            focusNode={selected && selected.kind !== "document" ? selected : null}
            preferredAgent={preferredAgent}
            onLit={onChatLit}
            onNeedsConnect={() => navigate("/connect")}
          />
        </div>
      </aside>
    </div>
  )
}
