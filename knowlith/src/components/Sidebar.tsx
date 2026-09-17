import { NavLink } from "react-router-dom"
import { Building2, ClipboardList, Eye, FolderOpen, Network, Plug, Search, Settings as SettingsIcon } from "lucide-react"
import { cn, initialsOf } from "@/lib/utils"
import { useApp } from "@/state/AppState"

/**
 * Flat job map in owner language.
 *
 * Labels answer "what am I doing", never the compiler's taxonomy.
 */
const LINKS: {
  to: string
  label: string
  Icon: typeof Building2
  end?: boolean
  badge?: "review" | "build"
  tour?: string
}[] = [
  { to: "/home", label: "Home", Icon: Building2, end: true, tour: "tour-home" },
  { to: "/review", label: "For review", Icon: ClipboardList, badge: "review", tour: "tour-review" },
  { to: "/build-quiz", label: "Confirm build", Icon: ClipboardList, badge: "build" },
  { to: "/browse", label: "Company knowledge", Icon: Search, tour: "tour-browse" },
  { to: "/brain", label: "Company brain", Icon: Network, tour: "tour-brain" },
  { to: "/connect", label: "AI assistants", Icon: Plug, tour: "tour-connect" },
  { to: "/sources", label: "Sources", Icon: FolderOpen, tour: "tour-sources" },
  { to: "/activity", label: "History", Icon: Eye, tour: "tour-history" },
  { to: "/settings", label: "Settings", Icon: SettingsIcon },
]

export function Sidebar() {
  const { sources, review, buildStatus, companyName, companyLogo } = useApp()
  const buildWaiting = buildStatus.quizPending && buildStatus.questionCount > 0

  return (
    <nav className="flex h-full w-full min-w-0 flex-col bg-bg" aria-label="Main">
      <div className="flex items-center gap-2 px-3 pb-3 pt-3">
        <div className="grid size-6 shrink-0 place-items-center overflow-hidden rounded-md bg-accent text-[11px] font-semibold text-on-accent">
          {companyLogo ? (
            <img src={companyLogo} alt="" className="size-full object-cover" />
          ) : (
            initialsOf(companyName)
          )}
        </div>
        <span className="truncate text-[13px] font-semibold text-ink">{companyName}</span>
      </div>

      <div className="flex flex-1 flex-col gap-0.5 px-2 pb-2">
        {LINKS.map(({ to, label, Icon, end, badge, tour }) => (
          <NavLink
            key={to}
            to={to}
            end={end}
            data-tour={tour}
            className={({ isActive }) =>
              cn(
                "flex items-center gap-2 rounded-md px-2 py-1.5 text-[13px]",
                isActive
                  ? "bg-accent-soft font-medium text-accent"
                  : (badge === "review" && review.length > 0) ||
                      (badge === "build" && buildWaiting)
                    ? "font-medium text-ink hover:bg-surface-3"
                    : "text-muted hover:bg-surface-3 hover:text-ink",
              )
            }
          >
            <Icon
              className={cn(
                "size-3.5 shrink-0",
                badge === "review" && review.length > 0
                  ? "text-pending"
                  : badge === "build" && buildWaiting
                    ? "text-pending"
                    : "text-faint",
              )}
            />
            <span className="flex-1 truncate">{label}</span>
            {badge === "review" && review.length > 0 ? (
              <span className="tabular rounded-sm bg-pending px-1.5 py-px text-[11px] font-semibold text-white">
                {review.length}
              </span>
            ) : badge === "build" && buildWaiting ? (
              <span className="tabular rounded-sm bg-pending px-1.5 py-px text-[11px] font-semibold text-white">
                {buildStatus.questionCount}
              </span>
            ) : null}
          </NavLink>
        ))}
      </div>

      {sources.length === 0 ? (
        <p className="border-t border-line px-3 py-2.5 text-[11.5px] leading-snug text-faint">
          No sources yet. Add a folder under Sources to start.
        </p>
      ) : null}
    </nav>
  )
}
