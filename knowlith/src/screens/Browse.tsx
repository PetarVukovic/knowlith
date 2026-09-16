import { useMemo, useState } from "react"
import { useNavigate, useSearchParams } from "react-router-dom"
import { Search } from "lucide-react"
import { KindIcon, StatusDot, kindMeta } from "@/components/Domain"
import type { ObjectKind, ObjectStatus } from "@/lib/types"
import { cn } from "@/lib/utils"
import { useApp } from "@/state/AppState"

type Filter = "all" | ObjectKind

type Row = {
  id: string
  title: string
  kind: ObjectKind
  status: ObjectStatus
  href: string
  blurb?: string
}

/**
 * Find a claim without living inside a sidebar tree.
 *
 * Filters are optional; search is the primary path (and Cmd+K still works).
 */
export function Browse() {
  const { objects, skills } = useApp()
  const navigate = useNavigate()
  const [params, setParams] = useSearchParams()
  const kindParam = params.get("kind")
  const filter: Filter =
    kindParam === "rule" || kindParam === "process" || kindParam === "skill" || kindParam === "fact"
      ? kindParam
      : "all"
  const setFilter = (next: Filter) => {
    const nextParams = new URLSearchParams(params)
    if (next === "all") nextParams.delete("kind")
    else nextParams.set("kind", next)
    setParams(nextParams, { replace: true })
  }
  const [query, setQuery] = useState("")

  const rows = useMemo(() => {
    const list: Row[] = []
    const skillIds = new Set(skills.map((s) => s.id))
    for (const o of objects) {
      // Skills arrive both in `objects` and `skills` — list them once, via
      // the skills endpoint, so the same procedure is not two cards.
      if (o.kind === "skill") continue
      // When an AI skill already exists for a process, the process stays in
      // Rules/Processes filters only — the Skills tab shows the runnable form.
      if (o.kind === "process") {
        const rest = o.id.includes(":") ? o.id.slice(o.id.indexOf(":") + 1) : o.id
        // Prefer the runnable AI skill in All / Skills; Processes tab still lists it.
        if (skillIds.has(`skill:${rest}`) && (filter === "all" || filter === "skill")) continue
      }
      const kind: ObjectKind = o.kind === "term" ? "fact" : o.kind
      list.push({
        id: o.id,
        title: o.title,
        kind,
        status: o.status,
        href: `/workspace/${encodeURIComponent(o.id)}`,
        blurb: o.body.replace(/^#+\s.*\n*/, "").trim().slice(0, 120),
      })
    }
    for (const s of skills) {
      list.push({
        id: s.id,
        title: s.name,
        kind: "skill",
        status: s.status,
        href: `/skills/${encodeURIComponent(s.id)}`,
        blurb: s.description,
      })
    }
    list.sort((a, b) => a.title.localeCompare(b.title))
    return list
  }, [objects, skills, filter])

  const q = query.trim().toLowerCase()
  const visible = rows.filter((row) => {
    if (filter !== "all" && row.kind !== filter) return false
    if (!q) return true
    return row.title.toLowerCase().includes(q) || (row.blurb?.toLowerCase().includes(q) ?? false)
  })

  return (
    <div className="mx-auto w-full max-w-[720px] px-4 py-8">
      <h1 className="text-[20px] font-semibold tracking-[-0.015em] text-ink">Company knowledge</h1>
      <p className="mt-1 max-w-[54ch] text-[13px] text-muted">
        Everything Knowlith learned that your company confirmed as current. Open an item to see the
        quote it came from — and try it in your AI.
      </p>

      <ul className="mt-4 grid gap-2 sm:grid-cols-2">
        {(["rule", "process", "term", "skill"] as const).map((kind) => (
          <li key={kind} className="rounded-lg border border-line bg-surface px-3 py-2.5">
            <div className="flex items-center gap-1.5 text-[12.5px] font-medium text-ink">
              <KindIcon kind={kind} />
              {kindMeta(kind).plural}
            </div>
            <p className="mt-1 text-[12px] leading-snug text-muted">{kindMeta(kind).meaning}</p>
          </li>
        ))}
      </ul>

      <label className="mt-5 flex h-9 items-center gap-2 rounded-md border border-line bg-surface-2 px-2.5">
        <Search className="size-3.5 shrink-0 text-faint" />
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="What do you want to find?"
          className="min-w-0 flex-1 bg-transparent text-[13px] text-ink outline-none placeholder:text-faint"
        />
      </label>

      <div className="mt-3 flex flex-wrap gap-1.5">
        {(
          [
            { id: "all" as const, label: "All" },
            { id: "rule" as const, label: "Rules" },
            { id: "process" as const, label: "Processes" },
            { id: "fact" as const, label: "Terms" },
            { id: "skill" as const, label: "AI skills" },
          ] as const
        ).map((f) => (
          <button
            key={f.id}
            type="button"
            onClick={() => setFilter(f.id)}
            className={cn(
              "rounded-md px-2.5 py-1 text-[12.5px]",
              filter === f.id
                ? "bg-accent-soft font-medium text-accent"
                : "text-muted hover:bg-surface-3 hover:text-ink",
            )}
          >
            {f.label}
          </button>
        ))}
      </div>

      {filter === "skill" ? (
        // The one place the rule is written down for the owner. It mirrors
        // the compiler exactly (crates/compiler/src/skills.rs): a skill is
        // drafted per confirmed process, from that process and what it
        // depends on, scored by its weakest source, and waits for approval.
        <div className="mt-5 rounded-lg border border-line bg-surface-2 px-4 py-3 text-[12.5px] leading-relaxed text-muted">
          <span className="font-medium text-ink">Where skills come from.</span> Each time you confirm a
          process, Knowlith drafts one skill for it — a task an assistant can carry out using that
          process and the rules and terms it depends on, and nothing else. Its confidence is that of the
          weakest thing it rests on. The draft waits under For review; no assistant sees it until you
          confirm it, and a process you have not confirmed produces no skill.
        </div>
      ) : null}

      {visible.length === 0 ? (
        <p className="mt-8 text-[13px] text-faint">
          {rows.length === 0
            ? "Nothing here yet. Confirm items under For review after a source is read."
            : filter === "skill"
              ? "No skills yet. Confirm a process under For review and one will be drafted from it."
              : "Nothing matches that search."}
        </p>
      ) : (
        <ul className="mt-5 grid gap-px overflow-hidden rounded-lg border border-line bg-line">
          {visible.map((row) => (
            <li key={row.id}>
              <button
                type="button"
                onClick={() => navigate(row.href)}
                className="flex w-full items-start gap-3 bg-surface px-3.5 py-2.5 text-left hover:bg-surface-2"
              >
                <KindIcon kind={row.kind} className="mt-0.5 shrink-0" />
                <span className="min-w-0 flex-1">
                  <span className="flex items-center gap-2">
                    <span className="truncate text-[13.5px] font-medium text-ink">{row.title}</span>
                    <StatusDot status={row.status} />
                  </span>
                  <span className="mt-0.5 block text-[12px] text-faint">{kindMeta(row.kind).label}</span>
                  {row.blurb ? (
                    <span className="mt-1 line-clamp-2 block text-[12.5px] text-muted">{row.blurb}</span>
                  ) : null}
                </span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}
