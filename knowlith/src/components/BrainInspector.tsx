import { useState } from "react"
import { useNavigate } from "react-router-dom"
import { AppWindow, ExternalLink, Loader2, Terminal } from "lucide-react"
import { Button } from "@/components/ui/button"
import { failed, tools as toolsApi } from "@/lib/api"
import { objectTryPrompt } from "@/lib/askAi"
import type { BrainAssistant, BrainEdge, BrainNode, ObjectKind } from "@/lib/types"
import { cn } from "@/lib/utils"

const LEGEND = [
  ["all", "All", "#94a3b8"],
  ["rule", "Rules", "#3b82f6"],
  ["process", "Processes", "#10b981"],
  ["skill", "Skills", "#f59e0b"],
  ["fact", "Terms", "#8b5cf6"],
  ["document", "Documents", "#8b979c"],
] as const

function asObjectKind(kind: string): ObjectKind {
  if (kind === "rule" || kind === "process" || kind === "skill" || kind === "term" || kind === "fact") {
    return kind
  }
  return "rule"
}

/**
 * Detail beside the brain map: filters, the selected node, its connections,
 * and buttons that open the owner's connected AI with a prepared question.
 */
export function BrainInspector({
  companyName,
  kindFilter,
  onKindFilter,
  selected,
  edges,
  nodes,
  assistants,
  onSelectId,
  onOpen,
}: {
  companyName: string
  kindFilter: string
  onKindFilter: (kind: string) => void
  selected: BrainNode | null
  edges: BrainEdge[]
  nodes: BrainNode[]
  assistants: BrainAssistant[]
  onSelectId: (id: string) => void
  onOpen: (node: BrainNode) => void
}) {
  const navigate = useNavigate()
  const [busy, setBusy] = useState<string | null>(null)
  const [note, setNote] = useState<string | null>(null)

  const titleOf = (id: string) => nodes.find((n) => n.id === id)?.title ?? id

  const around =
    selected == null
      ? []
      : edges
          .filter((e) => e.from === selected.id || e.to === selected.id)
          .map((e) =>
            e.from === selected.id
              ? { key: `${e.from}|${e.to}|${e.type}`, text: `${e.label} ${titleOf(e.to)}`, id: e.to, label: e.label }
              : { key: `${e.from}|${e.to}|${e.type}`, text: `${titleOf(e.from)} ${e.label} this`, id: e.from, label: e.label },
          )

  const ask = async (slug: string) => {
    if (!selected || selected.kind === "document") return
    const prompt = objectTryPrompt(asObjectKind(selected.kind), selected.title, companyName)
    setNote(null)
    setBusy(slug)
    const result = await toolsApi.try(slug, prompt)
    setBusy(null)
    if (failed(result)) {
      setNote(result.error)
      return
    }
    setNote(result.message)
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="shrink-0 border-b border-line px-3 py-2">
        <ul className="flex flex-wrap gap-1">
          {LEGEND.map(([id, label, color]) => {
            const active = kindFilter === id
            return (
              <li key={id}>
                <button
                  type="button"
                  onClick={() => onKindFilter(id)}
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
      </div>

      {selected ? (
        <div className="shrink-0 border-b border-line px-3 py-2.5">
          <div className="flex items-start gap-2">
            <span
              className="mt-1 size-2 shrink-0 rounded-full"
              style={{
                background:
                  LEGEND.find(([id]) => id === selected.kind || (id === "fact" && selected.kind === "term"))?.[2] ??
                  "#94a3b8",
              }}
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
              <Button size="sm" variant="subtle" onClick={() => onOpen(selected)}>
                <ExternalLink className="size-3.5" />
                Open
              </Button>
            ) : null}
          </div>

          {around.length > 0 ? (
            <ul className="mt-2.5 grid gap-1">
              <li className="text-[11px] font-medium uppercase tracking-wide text-faint">
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
            <p className="mt-2 text-[12px] text-faint">No connections yet.</p>
          )}

          {selected.kind !== "document" ? (
            <div className="mt-3 border-t border-line pt-3">
              <p className="text-[11.5px] font-medium text-ink">Ask in your AI</p>
              <p className="mt-0.5 text-[11.5px] leading-snug text-muted">
                Opens Claude Desktop, Codex or Terminal with this question ready — your MCP plugin reads Knowlith.
              </p>
              {assistants.length === 0 ? (
                <Button className="mt-2 w-full" size="sm" variant="primary" onClick={() => navigate("/connect")}>
                  Connect an assistant
                </Button>
              ) : (
                <ul className="mt-2 grid gap-1.5">
                  {assistants.map((tool) => {
                    const Icon = tool.surface === "terminal" ? Terminal : AppWindow
                    return (
                      <li key={tool.slug}>
                        <Button
                          className="w-full justify-start"
                          size="sm"
                          variant="default"
                          disabled={busy !== null}
                          onClick={() => void ask(tool.slug)}
                        >
                          {busy === tool.slug ? (
                            <Loader2 className="size-3.5 animate-spin" />
                          ) : (
                            <Icon className="size-3.5" />
                          )}
                          {tool.label}
                        </Button>
                      </li>
                    )
                  })}
                </ul>
              )}
              {note ? <p className="mt-2 text-[11.5px] leading-snug text-muted">{note}</p> : null}
            </div>
          ) : null}
        </div>
      ) : (
        <div className="shrink-0 border-b border-line px-3 py-4 text-[12.5px] leading-relaxed text-muted">
          Click a node to see what it connects to. Double-click to open the full entry.
        </div>
      )}

      <div className="min-h-0 flex-1" />
    </div>
  )
}
