import { useEffect, useMemo, useState } from "react"
import { useNavigate } from "react-router-dom"
import { AppWindow, ExternalLink, FileText, Loader2, Plug, Search, Terminal } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { failed, tools as toolsApi } from "@/lib/api"
import { objectTryPrompt } from "@/lib/askAi"
import { BRAIN_KIND_COLOR, BRAIN_LEGEND, brainDocumentLakeId, brainKindMatches } from "@/lib/brainGraph"
import type { AiTool, BrainAssistant, BrainEdge, BrainNode, ObjectKind } from "@/lib/types"
import { cn } from "@/lib/utils"

function asObjectKind(kind: string): ObjectKind {
  if (kind === "rule" || kind === "process" || kind === "skill" || kind === "term" || kind === "fact") {
    return kind
  }
  return "rule"
}

function kindLabel(kind: string): string {
  if (kind === "document") return "Document"
  if (kind === "term" || kind === "fact") return "Business term"
  return kind[0].toUpperCase() + kind.slice(1)
}

function kindColor(kind: string): string {
  if (kind === "term") return BRAIN_KIND_COLOR.fact
  return BRAIN_KIND_COLOR[kind] ?? BRAIN_KIND_COLOR.document
}

/**
 * Detail beside the brain map: the selected node, its connections, a list of
 * everything currently on the cortex, and buttons that open the owner's
 * connected AI with a prepared question.
 */
export function BrainInspector({
  companyName,
  kindFilter,
  selected,
  edges,
  nodes,
  assistants,
  onSelectId,
  onClear,
  onOpen,
  onOpenDocument,
}: {
  companyName: string
  kindFilter: string
  selected: BrainNode | null
  edges: BrainEdge[]
  nodes: BrainNode[]
  assistants: BrainAssistant[]
  onSelectId: (id: string) => void
  onClear: () => void
  onOpen: (node: BrainNode) => void
  onOpenDocument?: (documentId: string) => void
}) {
  const navigate = useNavigate()
  const [busy, setBusy] = useState<string | null>(null)
  const [note, setNote] = useState<string | null>(null)
  const [tools, setTools] = useState<AiTool[] | null>(null)
  const [query, setQuery] = useState("")

  useEffect(() => {
    void toolsApi.list().then(setTools)
  }, [])

  const titleOf = (id: string) => nodes.find((n) => n.id === id)?.title ?? id

  const around =
    selected == null
      ? []
      : edges
          .filter((e) => e.from === selected.id || e.to === selected.id)
          .map((e) =>
            e.from === selected.id
              ? { key: `${e.from}|${e.to}|${e.type}`, text: `${e.label} ${titleOf(e.to)}`, id: e.to }
              : { key: `${e.from}|${e.to}|${e.type}`, text: `${titleOf(e.from)} ${e.label} this`, id: e.from },
          )

  const listed = useMemo(() => {
    const q = query.trim().toLowerCase()
    return nodes
      .filter((n) => brainKindMatches(kindFilter, n.kind))
      .filter((n) => (q ? n.title.toLowerCase().includes(q) : true))
      .sort((a, b) => a.title.localeCompare(b.title))
  }, [nodes, kindFilter, query])

  const launch = async (slug: string, connected: boolean) => {
    if (!selected || selected.kind === "document") return
    if (!connected) {
      navigate("/connect")
      return
    }
    const prompt = objectTryPrompt(asObjectKind(selected.kind), selected.title, companyName)
    setNote(null)
    setBusy(slug)
    const result = await toolsApi.try(slug, prompt)
    setBusy(null)
    if (failed(result)) {
      setNote(result.error)
      if (result.error.includes("not connected") || result.error.includes("Repair")) {
        navigate("/connect")
      }
      return
    }
    setNote(result.message)
  }

  const toolRows = useMemo(() => {
    if (tools) {
      return tools
        .filter((t) => t.installed && t.launchSurface !== "missing")
        .map((t) => ({
          slug: t.slug,
          label: t.label,
          connected: t.connected,
          launchSurface: t.launchSurface,
        }))
    }
    return assistants
      .filter((a) => a.surface !== "missing")
      .map((a) => ({
        slug: a.slug,
        label: a.label,
        connected: a.connected,
        launchSurface: a.surface,
      }))
  }, [tools, assistants])

  const filterName = BRAIN_LEGEND.find(([id]) => id === kindFilter)?.[1]?.toLowerCase() ?? "items"

  return (
    <div className="flex h-full min-h-0 flex-col">
      {selected ? (
        <div className="shrink-0 border-b border-line px-3 py-3">
          <div className="flex items-start gap-2">
            <span
              className="mt-1.5 size-2 shrink-0 rounded-full"
              style={{ background: kindColor(selected.kind) }}
              aria-hidden
            />
            <div className="min-w-0 flex-1">
              <div className="truncate text-[13px] font-medium text-ink">{selected.title}</div>
              <div className="text-[11.5px] text-faint">
                {selected.kind === "document"
                  ? "Document — quoted by what it is joined to"
                  : kindLabel(selected.kind)}
              </div>
            </div>
            {selected.kind === "document" ? (
              <Button
                size="sm"
                variant="subtle"
                onClick={() => {
                  onOpenDocument?.(brainDocumentLakeId(selected.id))
                  onOpen(selected)
                }}
              >
                <FileText className="size-3.5" />
                Open snapshot
              </Button>
            ) : (
              <Button size="sm" variant="subtle" onClick={() => onOpen(selected)}>
                <ExternalLink className="size-3.5" />
                Open
              </Button>
            )}
          </div>

          {around.length > 0 ? (
            <ul className="mt-2.5 grid gap-0.5">
              <li className="px-1.5 text-[12px] text-faint">
                {around.length} {around.length === 1 ? "connection" : "connections"}
              </li>
              {around.map((a) => (
                <li key={a.key}>
                  <button
                    type="button"
                    className="w-full rounded-md px-1.5 py-1 text-left text-[12px] text-muted hover:bg-surface-2 hover:text-ink"
                    onClick={() => onSelectId(a.id)}
                  >
                    {a.text}
                  </button>
                </li>
              ))}
            </ul>
          ) : (
            <p className="mt-2 px-1.5 text-[12px] text-faint">No connections yet.</p>
          )}

          {selected.kind !== "document" ? (
            <div className="mt-3 border-t border-line pt-3">
              <p className="text-[12.5px] font-medium text-ink">Continue in your AI</p>
              <p className="mt-0.5 text-[11.5px] leading-snug text-muted">
                Desktop apps open in their own window. CLIs open a small separate terminal on your Mac or PC — not
                inside Knowlith. MCP must be connected first.
              </p>
              {toolRows.length === 0 ? (
                <Button className="mt-2 w-full" size="sm" variant="primary" onClick={() => navigate("/connect")}>
                  <Plug className="size-3.5" />
                  Set up AI assistants
                </Button>
              ) : (
                <ul className="mt-2 grid gap-1.5">
                  {toolRows.map((tool) => {
                    const Icon = tool.launchSurface === "terminal" ? Terminal : AppWindow
                    const connected = tool.connected
                    return (
                      <li key={tool.slug}>
                        <Button
                          className="w-full justify-start"
                          size="sm"
                          variant={connected ? "default" : "subtle"}
                          disabled={busy !== null}
                          onClick={() => void launch(tool.slug, connected)}
                        >
                          {busy === tool.slug ? (
                            <Loader2 className="size-3.5 animate-spin" />
                          ) : connected ? (
                            <Icon className="size-3.5" />
                          ) : (
                            <Plug className="size-3.5" />
                          )}
                          {connected ? `Open in ${tool.label}` : `Connect ${tool.label} first`}
                        </Button>
                      </li>
                    )
                  })}
                </ul>
              )}
              {note ? (
                <p className={cn("mt-2 text-[11.5px] leading-snug", note.includes("Opened") ? "text-confirmed" : "text-muted")}>
                  {note}
                </p>
              ) : null}
            </div>
          ) : null}

          <Button className="mt-3" size="sm" variant="ghost" onClick={onClear}>
            Show all
          </Button>
        </div>
      ) : (
        <div className="shrink-0 border-b border-line px-3 py-3 text-[12.5px] leading-relaxed text-muted">
          {kindFilter === "all" ? (
            <>Click a node on the brain to see what it connects to. Double-click to open the full entry.</>
          ) : (
            <>
              Showing {listed.length} {filterName}. Click one on the map or in the list.
            </>
          )}
        </div>
      )}

      <div className="flex min-h-0 flex-1 flex-col">
        <div className="shrink-0 px-3 pt-3">
          <div className="relative">
            <Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-faint" />
            <Input
              className="h-8 pl-8 text-[12.5px]"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Find on the brain…"
              aria-label="Find on the brain"
            />
          </div>
        </div>
        <ul className="min-h-0 flex-1 overflow-y-auto px-2 py-2">
          {listed.length === 0 ? (
            <li className="px-2 py-3 text-[12px] text-faint">Nothing matches.</li>
          ) : (
            listed.map((n) => {
              const active = selected?.id === n.id
              return (
                <li key={n.id}>
                  <button
                    type="button"
                    onClick={() => onSelectId(n.id)}
                    className={cn(
                      "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left",
                      active ? "bg-accent-soft text-ink" : "text-muted hover:bg-surface-2 hover:text-ink",
                    )}
                  >
                    <span
                      className="size-1.5 shrink-0 rounded-full"
                      style={{ background: kindColor(n.kind) }}
                      aria-hidden
                    />
                    <span className="min-w-0 flex-1 truncate text-[12.5px]">{n.title}</span>
                    <span className="shrink-0 text-[10.5px] text-faint">{kindLabel(n.kind)}</span>
                  </button>
                </li>
              )
            })
          )}
        </ul>
      </div>
    </div>
  )
}
