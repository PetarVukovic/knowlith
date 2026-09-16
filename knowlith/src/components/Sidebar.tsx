import { useMemo, useState } from "react"
import { NavLink, useNavigate, useParams } from "react-router-dom"
import {
  Building2,
  ChevronRight,
  Eye,
  FolderOpen,
  GitPullRequestArrow,
  Plug,
  Settings,
} from "lucide-react"
import { KindIcon, StatusDot, kindMeta } from "@/components/Domain"
import { Tooltip } from "@/components/ui/tooltip"
import type { ObjectKind } from "@/lib/types"
import { cn, initialsOf } from "@/lib/utils"
import { useApp } from "@/state/AppState"

const GROUPS: ObjectKind[] = ["fact", "rule", "process", "skill"]

/** Plain-language help, for the first session and for empty groups. */
const GROUP_HELP: Record<string, string> = {
  fact: "Terms, price lists and templates your documents refer to.",
  rule: "Decisions your company made: limits, deadlines, who approves what.",
  process: "Steps your team follows, in order.",
  skill: "Instructions an AI can carry out on your behalf.",
  sources: "The folders Knowlith reads. It never writes into them.",
}

function GroupHeader({
  label,
  help,
  open,
  onToggle,
  count,
}: {
  label: string
  help?: string
  open: boolean
  onToggle: () => void
  count: number
}) {
  return (
    <Tooltip content={help} side="right">
      <button
        type="button"
        onClick={onToggle}
        className="flex w-full items-center gap-1 rounded-sm px-1.5 py-1 text-left text-[12.5px] font-medium text-muted hover:bg-surface-3 hover:text-ink"
      >
        <ChevronRight className={cn("size-3 shrink-0 text-faint transition-transform", open && "rotate-90")} />
        <span className="flex-1 truncate">{label}</span>
        <span className="tabular text-[11px] text-faint">{count}</span>
      </button>
    </Tooltip>
  )
}

function GroupEmpty({ help }: { help: string }) {
  return <p className="px-1.5 py-1 pl-[26px] text-[11.5px] leading-snug text-faint">{help}</p>
}

export function Sidebar() {
  const { objects, skills, sources, review, companyName, companyLogo } = useApp()
  const navigate = useNavigate()
  const params = useParams()
  const [open, setOpen] = useState<Record<string, boolean>>({
    fact: true,
    rule: true,
    process: true,
    skill: true,
    sources: false,
  })

  const grouped = useMemo(() => {
    const map = new Map<ObjectKind, typeof objects>()
    for (const kind of GROUPS) map.set(kind, [])
    for (const o of objects) {
      if (o.kind === "term") map.get("fact")!.push(o)
      else map.get(o.kind)?.push(o)
    }
    return map
  }, [objects])

  const toggle = (key: string) => setOpen((s) => ({ ...s, [key]: !s[key] }))

  return (
    <nav className="flex h-full w-full min-w-0 flex-col bg-bg" aria-label="Context">
      <div className="flex items-center gap-2 px-3 pb-2 pt-3">
        <div className="grid size-6 shrink-0 place-items-center overflow-hidden rounded-md bg-accent text-[11px] font-semibold text-on-accent">
          {companyLogo ? (
            <img src={companyLogo} alt="" className="size-full object-cover" />
          ) : (
            initialsOf(companyName)
          )}
        </div>
        <span className="truncate text-[13px] font-semibold text-ink">{companyName}</span>
      </div>

      <div className="scroll-thin flex-1 overflow-y-auto px-2 pb-2">
        <div className="px-1.5 pb-1 pt-2 label-xs">Context</div>

        <NavLink
          to="/home"
          end
          className={({ isActive }) =>
            cn(
              "flex items-center gap-2 rounded-sm px-1.5 py-1 text-[12.5px]",
              isActive ? "bg-accent-soft font-medium text-accent" : "text-muted hover:bg-surface-3 hover:text-ink",
            )
          }
        >
          <Building2 className="size-3.5 shrink-0 text-faint" />
          Company overview
        </NavLink>

        {GROUPS.map((kind) => {
          const items = kind === "skill" ? [] : grouped.get(kind) ?? []
          const list =
            kind === "skill"
              ? skills.map((s) => ({ id: s.id, title: s.name, status: s.status, kind: "skill" as const }))
              : items.map((o) => ({ id: o.id, title: o.title, status: o.status, kind: o.kind }))
          return (
            <div key={kind} className="mt-1.5">
              <GroupHeader
                label={kindMeta(kind).plural}
                help={GROUP_HELP[kind]}
                open={open[kind]}
                onToggle={() => toggle(kind)}
                count={list.length}
              />
              {open[kind] && list.length === 0 ? <GroupEmpty help={GROUP_HELP[kind]} /> : null}
              {open[kind] ? (
                <ul className="mt-0.5 grid gap-px">
                  {list.map((item) => {
                    const target = item.kind === "skill" ? `/skills/${encodeURIComponent(item.id)}` : `/workspace/${encodeURIComponent(item.id)}`
                    const active = params.objectId === item.id || params.skillId === item.id
                    return (
                      <li key={item.id}>
                        <button
                          type="button"
                          onClick={() => navigate(target)}
                          className={cn(
                            "flex w-full items-center gap-2 rounded-sm py-1 pl-[26px] pr-1.5 text-left text-[12.5px]",
                            active
                              ? "bg-accent-soft font-medium text-accent"
                              : "text-muted hover:bg-surface-3 hover:text-ink",
                          )}
                        >
                          <KindIcon kind={item.kind} className="shrink-0" />
                          <span className="min-w-0 flex-1 truncate">{item.title}</span>
                          <StatusDot status={item.status} />
                        </button>
                      </li>
                    )
                  })}
                </ul>
              ) : null}
            </div>
          )
        })}

        <div className="mt-1.5">
          <GroupHeader
            label="Sources"
            help={GROUP_HELP.sources}
            open={open.sources}
            onToggle={() => toggle("sources")}
            count={sources.length}
          />
          {open.sources ? (
            <ul className="mt-0.5 grid gap-px">
              {sources.map((s) => (
                <li key={s.id}>
                  <button
                    type="button"
                    onClick={() => navigate("/sources")}
                    className="flex w-full items-center gap-2 rounded-sm py-1 pl-[26px] pr-1.5 text-left text-[12.5px] text-muted hover:bg-surface-3 hover:text-ink"
                  >
                    <FolderOpen className="size-3.5 shrink-0 text-faint" />
                    <span className="min-w-0 flex-1 truncate">{s.name}</span>
                  </button>
                </li>
              ))}
            </ul>
          ) : null}
        </div>
      </div>

      <div className="border-t border-line p-2">
        <NavLink
          to="/review"
          className={({ isActive }) =>
            cn(
              "flex items-center gap-2 rounded-sm px-1.5 py-1.5 text-[12.5px]",
              isActive
                ? "bg-accent-soft font-semibold text-accent"
                : review.length > 0
                  ? "font-medium text-ink hover:bg-surface-3"
                  : "text-muted hover:bg-surface-3 hover:text-ink",
            )
          }
        >
          <GitPullRequestArrow className={cn("size-3.5 shrink-0", review.length > 0 ? "text-pending" : "text-faint")} />
          <span className="flex-1">Changes</span>
          {review.length > 0 ? (
            <span className="tabular rounded-sm bg-pending px-1.5 py-px text-[11px] font-semibold text-white">
              {review.length}
            </span>
          ) : null}
        </NavLink>
        <NavLink
          to="/connect"
          className={({ isActive }) =>
            cn(
              "flex items-center gap-2 rounded-sm px-1.5 py-1.5 text-[12.5px]",
              isActive ? "bg-accent-soft font-medium text-accent" : "text-muted hover:bg-surface-3 hover:text-ink",
            )
          }
        >
          <Plug className="size-3.5 shrink-0 text-faint" />
          AI tools
        </NavLink>
        <NavLink
          to="/activity"
          className={({ isActive }) =>
            cn(
              "flex items-center gap-2 rounded-sm px-1.5 py-1.5 text-[12.5px]",
              isActive ? "bg-accent-soft font-medium text-accent" : "text-muted hover:bg-surface-3 hover:text-ink",
            )
          }
        >
          <Eye className="size-3.5 shrink-0 text-faint" />
          Activity
        </NavLink>
        <NavLink
          to="/sources"
          className={({ isActive }) =>
            cn(
              "flex items-center gap-2 rounded-sm px-1.5 py-1.5 text-[12.5px]",
              isActive ? "bg-accent-soft font-medium text-accent" : "text-muted hover:bg-surface-3 hover:text-ink",
            )
          }
        >
          <Settings className="size-3.5 shrink-0 text-faint" />
          Settings & sources
        </NavLink>
      </div>
    </nav>
  )
}
