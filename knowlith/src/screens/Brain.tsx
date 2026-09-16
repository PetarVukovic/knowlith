import { useCallback, useEffect, useRef, useState } from "react"
import { useNavigate } from "react-router-dom"
import {
  ExternalLink,
  Maximize2,
  Minimize2,
  Network,
  RefreshCw,
  Terminal,
  X,
} from "lucide-react"
import { AskAiPicker, type AskPickResult } from "@/components/AskAiPicker"
import { kindMeta } from "@/components/Domain"
import { LiveTerminal } from "@/components/LiveTerminal"
import { ResizeHandle, usePanelSize } from "@/components/Resizable"
import { Button } from "@/components/ui/button"
import { brain as brainApi } from "@/lib/api"
import { objectTryPrompt } from "@/lib/askAi"
import type { BrainEdge, BrainNode, CompanyBrain, ObjectKind } from "@/lib/types"
import { useApp } from "@/state/AppState"
import { cn } from "@/lib/utils"

type Pos = { x: number; y: number }
type Selection =
  | { kind: "node"; id: string }
  | { kind: "edge"; from: string; to: string; type: string }
  | null

type AgentSession = AskPickResult & {
  prompt: string
  about: string
}

const VIEW_W = 1200
const VIEW_H = 800

/**
 * Interactive company brain: drag nodes freely, click edges, pick which AI
 * answers. CLI assistants open a live PTY beside the map; desktop apps open
 * outside. Labels appear only for the hovered or selected node so the map
 * stays readable at production density.
 */
export function Brain() {
  const { companyName } = useApp()
  const navigate = useNavigate()
  // Narrower default so the map keeps most of the viewport; drag to grow.
  const side = usePanelSize("brain-side-v2", 340, 260, 640)
  const [data, setData] = useState<CompanyBrain | null>(null)
  const [positions, setPositions] = useState<Map<string, Pos>>(new Map())
  const [selection, setSelection] = useState<Selection>(null)
  const [hoverId, setHoverId] = useState<string | null>(null)
  const [session, setSession] = useState<AgentSession | null>(null)
  const [hint, setHint] = useState<string | null>(null)
  const [pan, setPan] = useState({ x: 0, y: 0 })
  const [zoom, setZoom] = useState(1)
  const [fullscreen, setFullscreen] = useState(false)
  const [pickerOpen, setPickerOpen] = useState(false)
  const [pendingAsk, setPendingAsk] = useState<{
    prompt: string
    about: string
    nodeId: string
  } | null>(null)

  const drag = useRef<{
    mode: "node" | "pan"
    id?: string
    /** Client pixels for pan; SVG world for node. */
    startX: number
    startY: number
    origX: number
    origY: number
    moved: boolean
  } | null>(null)
  const svgRef = useRef<SVGSVGElement>(null)

  const [kindFilter, setKindFilter] = useState<ObjectKind | "all">("all")

  const load = useCallback(async () => {
    const next = await brainApi.get()
    setData(next)
    setPositions((prev) => {
      const layout = layoutNodes(next.nodes)
      const merged = new Map(layout)
      for (const [id, pos] of prev) {
        if (merged.has(id)) merged.set(id, pos)
      }
      return merged
    })
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

  // Live ask needs map + terminal without the nav chrome — leaving the shell
  // on squeezes both into a third of the screen.
  useEffect(() => {
    if (session?.embedded) setFullscreen(true)
  }, [session?.embedded])

  /** Screen → graph world (accounts for pan + zoom on the inner group). */
  const clientToWorld = (clientX: number, clientY: number): Pos => {
    const svg = svgRef.current
    if (!svg) return { x: 0, y: 0 }
    const rect = svg.getBoundingClientRect()
    const viewX = ((clientX - rect.left) / rect.width) * VIEW_W
    const viewY = ((clientY - rect.top) / rect.height) * VIEW_H
    return {
      x: (viewX - pan.x) / zoom,
      y: (viewY - pan.y) / zoom,
    }
  }

  const viewScale = () => {
    const svg = svgRef.current
    if (!svg) return 1
    return VIEW_W / svg.getBoundingClientRect().width
  }

  const promptForNode = (node: BrainNode) => {
    const kind = (["rule", "process", "term", "skill", "fact"].includes(node.kind)
      ? node.kind
      : "fact") as ObjectKind
    return objectTryPrompt(kind, node.title, companyName)
  }

  const askAboutNode = (node: BrainNode) => {
    setPendingAsk({
      prompt: promptForNode(node),
      about: node.title,
      nodeId: node.id,
    })
    setPickerOpen(true)
  }

  const selectNode = (id: string, autoAsk: boolean) => {
    setSelection({ kind: "node", id })
    const node = data?.nodes.find((n) => n.id === id)
    if (autoAsk && node) askAboutNode(node)
  }

  const onPointerMove = (e: React.PointerEvent) => {
    const d = drag.current
    if (!d) return
    if (d.mode === "node" && d.id) {
      const cur = clientToWorld(e.clientX, e.clientY)
      const dx = cur.x - d.startX
      const dy = cur.y - d.startY
      if (Math.abs(dx) + Math.abs(dy) > 3) d.moved = true
      setPositions((prev) => {
        const next = new Map(prev)
        next.set(d.id!, { x: d.origX + dx, y: d.origY + dy })
        return next
      })
    } else if (d.mode === "pan") {
      const scale = viewScale()
      const dx = (e.clientX - d.startX) * scale
      const dy = (e.clientY - d.startY) * scale
      if (Math.abs(dx) + Math.abs(dy) > 3) d.moved = true
      setPan({ x: d.origX + dx, y: d.origY + dy })
    }
  }

  const onPointerUp = () => {
    drag.current = null
  }

  const selectedNode =
    selection?.kind === "node" ? data?.nodes.find((n) => n.id === selection.id) : null
  const selectedEdge =
    selection?.kind === "edge"
      ? data?.edges.find(
          (e) =>
            e.from === selection.from && e.to === selection.to && e.type === selection.type,
        )
      : null

  const related = selectedNode
    ? (data?.edges ?? []).filter(
        (e) => e.from === selectedNode.id || e.to === selectedNode.id,
      )
    : []

  const liveSession = Boolean(session?.embedded)

  return (
    <div
      data-fill-screen
      className={cn(
        "flex min-h-0 w-full bg-bg",
        fullscreen ? "fixed inset-0 z-50" : "h-full",
      )}
    >
      <div
        className={cn(
          "flex min-w-0 flex-1 flex-col",
          fullscreen || liveSession ? "px-0 pt-0" : "px-3 pt-3",
        )}
      >
        <div
          className={cn(
            "flex shrink-0 items-center gap-2",
            fullscreen || liveSession
              ? "border-b border-line bg-surface px-3 py-1.5"
              : "px-1 pb-2",
          )}
        >
          <h1 className="flex min-w-0 items-center gap-1.5 text-[14px] font-semibold text-ink">
            <Network className="size-3.5 shrink-0 text-faint" />
            <span className="truncate">Company brain</span>
          </h1>
          {!liveSession && !fullscreen ? (
            <p className="hidden min-w-0 flex-1 truncate text-[12px] text-muted lg:block">
              Click a node · Ask AI opens live beside the map
            </p>
          ) : (
            <span className="min-w-0 flex-1" />
          )}
          <div className="flex shrink-0 items-center gap-1">
            <Button
              variant="ghost"
              size="sm"
              onClick={() => {
                setZoom(1)
                setPan({ x: 0, y: 0 })
                if (data) setPositions(layoutNodes(data.nodes))
              }}
            >
              Reset
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

        {hint && !liveSession ? (
          <p className="px-1 pb-1 text-[12px] text-muted">{hint}</p>
        ) : null}

        <div
          className={cn(
            "relative min-h-0 flex-1 overflow-hidden bg-surface",
            fullscreen || liveSession ? "border-0" : "rounded-lg border border-line",
          )}
        >
          {!data || data.nodes.length === 0 ? (
            <div className="grid h-full place-items-center p-8 text-center text-[13px] text-muted">
              No approved knowledge yet. Confirm items under For review and they
              appear here.
            </div>
          ) : (
            <svg
              ref={svgRef}
              viewBox={`0 0 ${VIEW_W} ${VIEW_H}`}
              className="h-full w-full touch-none"
              role="img"
              aria-label="Interactive company knowledge graph"
              onPointerMove={onPointerMove}
              onPointerUp={onPointerUp}
              onPointerLeave={() => {
                onPointerUp()
                setHoverId(null)
              }}
              onWheel={(e) => {
                e.preventDefault()
                setZoom((z) => Math.min(2.4, Math.max(0.45, z * (e.deltaY > 0 ? 0.92 : 1.08))))
              }}
              onPointerDown={(e) => {
                if (e.target !== e.currentTarget && (e.target as Element).tagName !== "svg") return
                drag.current = {
                  mode: "pan",
                  startX: e.clientX,
                  startY: e.clientY,
                  origX: pan.x,
                  origY: pan.y,
                  moved: false,
                }
                e.currentTarget.setPointerCapture(e.pointerId)
              }}
            >
              <g transform={`translate(${pan.x} ${pan.y}) scale(${zoom})`}>
                <BrainSilhouette />
                {(data.edges ?? []).map((edge) => {
                  const a = positions.get(edge.from)
                  const b = positions.get(edge.to)
                  if (!a || !b) return null
                  const active =
                    selection?.kind === "edge" &&
                    selection.from === edge.from &&
                    selection.to === edge.to &&
                    selection.type === edge.type
                  const touched =
                    selection?.kind === "node" &&
                    (selection.id === edge.from || selection.id === edge.to)
                  return (
                    <g key={`${edge.from}-${edge.to}-${edge.type}`}>
                      <line
                        x1={a.x}
                        y1={a.y}
                        x2={b.x}
                        y2={b.y}
                        stroke="transparent"
                        strokeWidth={14}
                        className="cursor-pointer"
                        onPointerDown={(e) => {
                          e.stopPropagation()
                          setSelection({
                            kind: "edge",
                            from: edge.from,
                            to: edge.to,
                            type: edge.type,
                          })
                        }}
                      />
                      <line
                        x1={a.x}
                        y1={a.y}
                        x2={b.x}
                        y2={b.y}
                        stroke={
                          active
                            ? "var(--color-accent, #2563eb)"
                            : touched
                              ? "#94a3b8"
                              : "var(--color-line, #d4d4d4)"
                        }
                        strokeWidth={active ? 2.5 : touched ? 1.8 : 1.2}
                        className="pointer-events-none"
                      />
                    </g>
                  )
                })}

                {(data.nodes ?? []).map((node) => {
                  const pos = positions.get(node.id)
                  if (!pos) return null
                  const matches =
                    kindFilter === "all" ||
                    node.kind === kindFilter ||
                    (kindFilter === "fact" && (node.kind === "term" || node.kind === "fact"))
                  const active = selection?.kind === "node" && selection.id === node.id
                  const hovered = hoverId === node.id
                  const showLabel = active || hovered
                  return (
                    <g
                      key={node.id}
                      transform={`translate(${pos.x}, ${pos.y})`}
                      opacity={matches ? 1 : 0.18}
                      className="cursor-grab active:cursor-grabbing"
                      onPointerEnter={() => setHoverId(node.id)}
                      onPointerLeave={() =>
                        setHoverId((cur) => (cur === node.id ? null : cur))
                      }
                      onPointerDown={(e) => {
                        e.stopPropagation()
                        const p = positions.get(node.id) ?? { x: 0, y: 0 }
                        const start = clientToWorld(e.clientX, e.clientY)
                        drag.current = {
                          mode: "node",
                          id: node.id,
                          startX: start.x,
                          startY: start.y,
                          origX: p.x,
                          origY: p.y,
                          moved: false,
                        }
                        e.currentTarget.setPointerCapture(e.pointerId)
                      }}
                      onPointerUp={(e) => {
                        const d = drag.current
                        const wasDrag = d?.moved
                        drag.current = null
                        if (!wasDrag) {
                          e.stopPropagation()
                          selectNode(node.id, true)
                        }
                      }}
                    >
                      <circle
                        r={active ? 20 : hovered ? 17 : 15}
                        fill={kindFill(node.kind)}
                        stroke={
                          active
                            ? "var(--color-accent, #2563eb)"
                            : hovered
                              ? "#cbd5e1"
                              : "#fff"
                        }
                        strokeWidth={active ? 3 : 2}
                      />
                      {showLabel ? (
                        <g className="pointer-events-none">
                          <rect
                            x={-72}
                            y={24}
                            width={144}
                            height={22}
                            rx={4}
                            fill="var(--color-surface, #fff)"
                            stroke="var(--color-line, #d4d4d4)"
                            strokeWidth={1}
                            opacity={0.96}
                          />
                          <text
                            y={39}
                            textAnchor="middle"
                            style={{
                              fill: "var(--color-ink, #171717)",
                              fontSize: 11,
                              fontWeight: active ? 600 : 500,
                            }}
                          >
                            {truncate(node.title, 22)}
                          </text>
                        </g>
                      ) : null}
                      <title>{node.title}</title>
                    </g>
                  )
                })}
              </g>
            </svg>
          )}
        </div>
      </div>

      <ResizeHandle
        panel={side}
        edge="end"
        label="Resize live terminal panel"
        className="hidden md:block"
      />

      <aside
        className="flex shrink-0 flex-col border-l border-line bg-bg"
        style={{ width: side.width }}
      >
        {liveSession && session ? (
          <div className="min-h-0 flex-1">
            <LiveTerminal
              key={`${session.slug}:${session.prompt}`}
              app={session.slug}
              prompt={session.prompt}
              label={`${session.label} · ${session.about}`}
              onClose={() => setSession(null)}
            />
          </div>
        ) : (
          <>
            <div className="border-b border-line px-3 py-2.5">
              <div className="text-[11px] font-medium uppercase tracking-wide text-faint">
                Ask AI
              </div>
              <p className="mt-0.5 text-[12px] text-muted">
                Pick a node, then an assistant — live CLI opens here.
              </p>
            </div>

            {data ? (
              <div className="border-b border-line px-3 py-2.5">
                <div className="text-[11px] font-medium uppercase tracking-wide text-faint">
                  Index
                </div>
                <ul className="mt-2 space-y-1">
                  {(
                    [
                      ["all", "Everything", "#94a3b8"],
                      ["rule", "Rules", "#3b82f6"],
                      ["process", "Processes", "#10b981"],
                      ["skill", "AI skills", "#f59e0b"],
                      ["fact", "Terms & facts", "#8b5cf6"],
                    ] as const
                  ).map(([id, label, color]) => {
                    const count =
                      id === "all"
                        ? data.nodes.length
                        : data.nodes.filter((n) =>
                            id === "fact"
                              ? n.kind === "fact" || n.kind === "term"
                              : n.kind === id,
                          ).length
                    const active = kindFilter === id
                    return (
                      <li key={id}>
                        <button
                          type="button"
                          onClick={() => setKindFilter(id)}
                          className={cn(
                            "flex w-full items-center gap-2 rounded-md px-1.5 py-1 text-left text-[12.5px]",
                            active ? "bg-accent-soft text-ink" : "text-muted hover:bg-surface-2",
                          )}
                        >
                          <span
                            className="size-2.5 shrink-0 rounded-full"
                            style={{ background: color }}
                            aria-hidden
                          />
                          <span className="min-w-0 flex-1 font-medium">{label}</span>
                          <span className="tabular-nums text-faint">{count}</span>
                        </button>
                      </li>
                    )
                  })}
                </ul>
              </div>
            ) : null}

            <div className="min-h-0 flex-1 overflow-y-auto">
              {!selection ? (
                <p className="px-3 py-5 text-[13px] text-faint">
                  Select a node or an edge on the map.
                </p>
              ) : null}

              {selectedNode ? (
                <div className="border-b border-line px-3 py-3">
                  <div className="text-[11px] uppercase tracking-wide text-faint">
                    {kindMeta(
                      (["rule", "process", "term", "skill", "fact"].includes(
                        selectedNode.kind,
                      )
                        ? selectedNode.kind
                        : "fact") as ObjectKind,
                    ).label}
                  </div>
                  <h2 className="mt-1 text-[15px] font-semibold text-ink">
                    {selectedNode.title}
                  </h2>
                  <p className="mt-1.5 text-[12.5px] text-muted">
                    {kindMeta(
                      (["rule", "process", "term", "skill", "fact"].includes(
                        selectedNode.kind,
                      )
                        ? selectedNode.kind
                        : "fact") as ObjectKind,
                    ).meaning}
                  </p>
                  {related.length > 0 ? (
                    <ul className="mt-3 grid gap-1.5">
                      {related.map((e) => {
                        const otherId = e.from === selectedNode.id ? e.to : e.from
                        const other = data?.nodes.find((n) => n.id === otherId)
                        return (
                          <li key={`${e.from}-${e.to}-${e.type}`}>
                            <button
                              type="button"
                              className="w-full rounded-md px-2 py-1.5 text-left text-[12px] text-muted hover:bg-surface-3 hover:text-ink"
                              onClick={() => selectNode(otherId, true)}
                            >
                              <span className="text-faint">{edgeLabel(e.type)}</span>{" "}
                              {other?.title ?? otherId}
                            </button>
                          </li>
                        )
                      })}
                    </ul>
                  ) : (
                    <p className="mt-3 text-[12px] text-faint">No links yet.</p>
                  )}
                  <div className="mt-3 flex flex-wrap gap-2">
                    <Button
                      size="sm"
                      variant="default"
                      onClick={() =>
                        navigate(
                          selectedNode.kind === "skill"
                            ? `/skills/${encodeURIComponent(selectedNode.id)}`
                            : `/workspace/${encodeURIComponent(selectedNode.id)}`,
                        )
                      }
                    >
                      <ExternalLink className="size-3.5" />
                      Open
                    </Button>
                    <Button
                      size="sm"
                      variant="primary"
                      onClick={() => askAboutNode(selectedNode)}
                    >
                      <Terminal className="size-3.5" />
                      Ask AI…
                    </Button>
                  </div>
                </div>
              ) : null}

              {selectedEdge && data ? (
                <EdgeDetail
                  edge={selectedEdge}
                  nodes={data.nodes}
                  onOpen={(id) => selectNode(id, true)}
                />
              ) : null}

              {session ? (
                <div className="border-t border-line px-3 py-3 text-[12.5px]">
                  <div className="flex items-start justify-between gap-2">
                    <div>
                      <div className="font-medium text-ink">{session.label} opened</div>
                      <p className="mt-1 text-muted">{session.message}</p>
                      <p className="mt-2 text-[12px] text-faint">
                        {session.surface === "terminal"
                          ? "A real Terminal window is running that command — this panel is only a record."
                          : "The desktop app opened outside Knowlith with the question ready."}
                      </p>
                    </div>
                    <button
                      type="button"
                      className="text-faint hover:text-ink"
                      onClick={() => setSession(null)}
                      aria-label="Dismiss"
                    >
                      <X className="size-3.5" />
                    </button>
                  </div>
                  <p className="mt-3 text-[11px] uppercase tracking-wide text-faint">
                    Question
                  </p>
                  <p className="mt-1 italic text-muted">“{session.prompt}”</p>
                </div>
              ) : selection ? (
                <p className="px-3 py-3 text-[12.5px] text-faint">
                  Choose an AI above to open a live session.
                </p>
              ) : null}
            </div>

            {data && data.assistants.length > 0 ? (
              <div className="border-t border-line px-3 py-2 text-[11.5px] text-faint">
                Connected: {data.assistants.map((a) => a.label).join(", ")}
              </div>
            ) : (
              <div className="border-t border-line px-3 py-2 text-[11.5px] text-pending">
                No AI assistant connected — connect one to ask from the map.
              </div>
            )}
          </>
        )}
      </aside>

      <AskAiPicker
        open={pickerOpen}
        onOpenChange={setPickerOpen}
        prompt={pendingAsk?.prompt ?? ""}
        about={pendingAsk?.about}
        embedded
        onNeedsConnect={() => navigate("/connect")}
        onLaunched={(result) => {
          if (!pendingAsk) return
          setHint(result.message)
          setSession({
            ...result,
            prompt: pendingAsk.prompt,
            about: pendingAsk.about,
          })
        }}
      />
    </div>
  )
}

function EdgeDetail({
  edge,
  nodes,
  onOpen,
}: {
  edge: BrainEdge
  nodes: BrainNode[]
  onOpen: (id: string) => void
}) {
  const from = nodes.find((n) => n.id === edge.from)
  const to = nodes.find((n) => n.id === edge.to)
  return (
    <div className="border-b border-line px-3 py-3">
      <div className="text-[11px] uppercase tracking-wide text-faint">Connection</div>
      <h2 className="mt-1 text-[15px] font-semibold text-ink">{edgeLabel(edge.type)}</h2>
      <p className="mt-1.5 text-[12.5px] text-muted">{edgeMeaning(edge.type)}</p>
      <div className="mt-3 grid gap-2">
        <button
          type="button"
          className="rounded-md border border-line bg-surface px-3 py-2 text-left text-[12.5px] hover:bg-surface-2"
          onClick={() => onOpen(edge.from)}
        >
          <span className="text-faint">From</span>
          <div className="font-medium text-ink">{from?.title ?? edge.from}</div>
        </button>
        <button
          type="button"
          className="rounded-md border border-line bg-surface px-3 py-2 text-left text-[12.5px] hover:bg-surface-2"
          onClick={() => onOpen(edge.to)}
        >
          <span className="text-faint">To</span>
          <div className="font-medium text-ink">{to?.title ?? edge.to}</div>
        </button>
      </div>
    </div>
  )
}

function BrainSilhouette() {
  const cx = VIEW_W / 2
  const cy = VIEW_H / 2 + 10
  // Top-down brain outline: two lobes, midline fissure, rounded occiput.
  const d = [
    `M ${cx} ${cy - 268}`,
    `C ${cx - 70} ${cy - 275}, ${cx - 200} ${cy - 240}, ${cx - 310} ${cy - 140}`,
    `C ${cx - 380} ${cy - 40}, ${cx - 390} ${cy + 80}, ${cx - 340} ${cy + 180}`,
    `C ${cx - 290} ${cy + 255}, ${cx - 160} ${cy + 285}, ${cx - 40} ${cy + 250}`,
    `C ${cx - 12} ${cy + 220}, ${cx + 12} ${cy + 220}, ${cx + 40} ${cy + 250}`,
    `C ${cx + 160} ${cy + 285}, ${cx + 290} ${cy + 255}, ${cx + 340} ${cy + 180}`,
    `C ${cx + 390} ${cy + 80}, ${cx + 380} ${cy - 40}, ${cx + 310} ${cy - 140}`,
    `C ${cx + 200} ${cy - 240}, ${cx + 70} ${cy - 275}, ${cx} ${cy - 268}`,
    "Z",
  ].join(" ")
  const fissure = `M ${cx} ${cy - 200} C ${cx - 8} ${cy - 40}, ${cx + 8} ${cy + 60}, ${cx} ${cy + 210}`
  return (
    <g aria-hidden className="pointer-events-none">
      <path
        d={d}
        fill="var(--color-accent-soft, #eff6ff)"
        fillOpacity={0.55}
        stroke="var(--color-line, #d4d4d4)"
        strokeWidth={1.5}
      />
      <path
        d={fissure}
        fill="none"
        stroke="var(--color-line, #d4d4d4)"
        strokeWidth={1.2}
        strokeDasharray="4 6"
        opacity={0.7}
      />
      <ellipse
        cx={cx - 150}
        cy={cy - 10}
        rx={150}
        ry={170}
        fill="var(--color-accent, #2563eb)"
        fillOpacity={0.03}
      />
      <ellipse
        cx={cx + 150}
        cy={cy - 10}
        rx={150}
        ry={170}
        fill="var(--color-accent, #2563eb)"
        fillOpacity={0.03}
      />
    </g>
  )
}

/**
 * Packs nodes into a brain-shaped field (two lobes + denser core), not a ring.
 *
 * Placement is deterministic from the node id so refresh does not reshuffle
 * the map under the owner's hands.
 */
function layoutNodes(nodes: BrainNode[]): Map<string, Pos> {
  const map = new Map<string, Pos>()
  const n = nodes.length
  if (n === 0) return map

  const cx = VIEW_W / 2
  const cy = VIEW_H / 2 + 10
  const rx = Math.min(320, 120 + n * 4)
  const ry = Math.min(250, 100 + n * 3.2)

  const lobeOf = (kind: string): number => {
    switch (kind) {
      case "rule":
      case "term":
        return -1
      case "process":
      case "skill":
        return 1
      default:
        return 0
    }
  }
  const ordered = [...nodes].sort((a, b) => {
    const la = lobeOf(a.kind) - lobeOf(b.kind)
    if (la !== 0) return la
    return a.title.localeCompare(b.title)
  })

  const golden = Math.PI * (3 - Math.sqrt(5))
  ordered.forEach((node, i) => {
    const lobe = lobeOf(node.kind)
    const t = (i + 0.5) / n
    const r = Math.sqrt(t)
    const theta = i * golden + (lobe < 0 ? -0.35 : lobe > 0 ? 0.35 : 0)
    let ux = r * Math.cos(theta)
    let uy = r * Math.sin(theta)

    ux = ux * 0.72 + lobe * 0.38
    const fissure = Math.tanh(ux * 10) * 0.1
    ux += fissure

    const midWidth = 1 + 0.22 * Math.cos(uy * Math.PI * 0.9)
    const frontal = 1 - 0.14 * Math.max(0, -uy)
    const occipital = 1 - 0.06 * Math.max(0, uy)

    const j = jitter(node.id)
    const x = cx + (ux * midWidth * frontal + j.x * 0.04) * rx
    const y = cy + (uy * occipital + j.y * 0.04) * ry
    map.set(node.id, { x, y })
  })
  return map
}

/** [-1,1] pair from id — same id always lands in the same place. */
function jitter(id: string): Pos {
  let h = 2166136261
  for (let i = 0; i < id.length; i++) {
    h ^= id.charCodeAt(i)
    h = Math.imul(h, 16777619)
  }
  const x = ((h >>> 0) % 2000) / 1000 - 1
  const y = ((h >>> 16) % 2000) / 1000 - 1
  return { x, y }
}

function kindFill(kind: string): string {
  switch (kind) {
    case "rule":
      return "#3b82f6"
    case "process":
      return "#10b981"
    case "skill":
      return "#f59e0b"
    case "term":
    case "fact":
      return "#8b5cf6"
    default:
      return "#94a3b8"
  }
}

function edgeLabel(type: string): string {
  switch (type) {
    case "depends_on":
      return "Depends on"
    case "used_by":
      return "Used by"
    case "derived_from":
      return "Derived from"
    case "conflicts_with":
      return "Conflicts with"
    default:
      return type
  }
}

function edgeMeaning(type: string): string {
  switch (type) {
    case "depends_on":
      return "This claim needs the other to stay true."
    case "used_by":
      return "If you change this, the other is affected."
    case "derived_from":
      return "This was computed from the other claim."
    case "conflicts_with":
      return "Two documents disagree — you decide which is current."
    default:
      return "A link between two confirmed claims."
  }
}

function truncate(text: string, max: number): string {
  return text.length <= max ? text : `${text.slice(0, max - 1)}…`
}
