import { Command } from "cmdk"
import { useNavigate } from "react-router-dom"
import { Eye, FolderOpen, GitPullRequestArrow, LayoutGrid, Moon, Plug, RefreshCw, Sparkles, Sun, Wrench } from "lucide-react"
import { KindIcon, kindMeta } from "@/components/Domain"
import { useApp } from "@/state/AppState"

/**
 * One entry point for everything. An owner types what they call the thing
 * ("popust", "jamstvo") and lands on the object, not on a search results page.
 */
export function CommandPalette() {
  const { paletteOpen, setPaletteOpen, objects, skills, review, setTheme, setMode, mode, resetOnboarding, setFirstRun } =
    useApp()
  const navigate = useNavigate()

  const go = (path: string) => {
    setPaletteOpen(false)
    navigate(path)
  }

  return (
    <Command.Dialog
      open={paletteOpen}
      onOpenChange={setPaletteOpen}
      label="Search and commands"
      overlayClassName="fixed inset-0 z-50 bg-black/25 backdrop-blur-[1px]"
      contentClassName="fixed left-1/2 top-[14vh] z-50 w-[min(600px,calc(100vw-32px))] -translate-x-1/2 overflow-hidden rounded-xl border border-line bg-surface shadow-k-lg"
    >
      <Command loop className="[&_[cmdk-input-wrapper]]:border-b [&_[cmdk-input-wrapper]]:border-line">
        <Command.Input
          placeholder="Search rules, processes, terms, skills…"
          className="h-11 w-full border-b border-line bg-transparent px-4 text-[13.5px] text-ink outline-none placeholder:text-faint"
        />
        <Command.List className="scroll-thin max-h-[54vh] overflow-y-auto p-1.5">
          <Command.Empty className="px-3 py-6 text-center text-[12.5px] text-muted">
            Nothing matches. If it should be here, add the folder it lives in.
          </Command.Empty>

          <Command.Group heading="Go to" className="[&_[cmdk-group-heading]]:px-2 [&_[cmdk-group-heading]]:py-1.5 [&_[cmdk-group-heading]]:label-xs">
            <PaletteItem onSelect={() => go("/workspace")} icon={<LayoutGrid />} label="Company overview" />
            <PaletteItem
              onSelect={() => go("/review")}
              icon={<GitPullRequestArrow />}
              label="Changes waiting for review"
              hint={review.length > 0 ? `${review.length}` : undefined}
            />
            <PaletteItem onSelect={() => go("/sources")} icon={<FolderOpen />} label="Sources" />
            <PaletteItem onSelect={() => go("/connect")} icon={<Plug />} label="AI tools" />
            <PaletteItem onSelect={() => go("/activity")} icon={<Eye />} label="Activity" />
            <PaletteItem onSelect={() => go("/discovery")} icon={<RefreshCw />} label="Last discovery report" />
            <PaletteItem
              onSelect={() => {
                setFirstRun(null)
                resetOnboarding()
                go("/onboarding")
              }}
              icon={<Sparkles />}
              label="Run setup again"
              hint="from the start"
            />
          </Command.Group>

          <Command.Group heading="Context" className="[&_[cmdk-group-heading]]:px-2 [&_[cmdk-group-heading]]:py-1.5 [&_[cmdk-group-heading]]:label-xs">
            {objects.map((o) => (
              <PaletteItem
                key={o.id}
                value={`${o.title} ${o.id} ${kindMeta(o.kind).label}`}
                onSelect={() => go(`/workspace/${encodeURIComponent(o.id)}`)}
                icon={<KindIcon kind={o.kind} />}
                label={o.title}
                hint={kindMeta(o.kind).label}
              />
            ))}
            {skills.map((s) => (
              <PaletteItem
                key={s.id}
                value={`${s.name} ${s.id} skill`}
                onSelect={() => go(`/skills/${encodeURIComponent(s.id)}`)}
                icon={<Wrench />}
                label={s.name}
                hint="Skill"
              />
            ))}
          </Command.Group>

          <Command.Group heading="Commands" className="[&_[cmdk-group-heading]]:px-2 [&_[cmdk-group-heading]]:py-1.5 [&_[cmdk-group-heading]]:label-xs">
            <PaletteItem
              onSelect={() => {
                setMode(mode === "simple" ? "engineer" : "simple")
                setPaletteOpen(false)
              }}
              icon={<Wrench />}
              label={mode === "simple" ? "Switch to engineer mode" : "Switch to simple mode"}
            />
            <PaletteItem onSelect={() => { setTheme("light"); setPaletteOpen(false) }} icon={<Sun />} label="Light appearance" />
            <PaletteItem onSelect={() => { setTheme("dark"); setPaletteOpen(false) }} icon={<Moon />} label="Dark appearance" />
          </Command.Group>
        </Command.List>
      </Command>
    </Command.Dialog>
  )
}

function PaletteItem({
  icon,
  label,
  hint,
  onSelect,
  value,
}: {
  icon: React.ReactNode
  label: string
  hint?: string
  onSelect: () => void
  value?: string
}) {
  return (
    <Command.Item
      value={value ?? label}
      onSelect={onSelect}
      className="flex cursor-pointer items-center gap-2.5 rounded-md px-2 py-2 text-[13px] text-ink data-[selected=true]:bg-surface-3 [&_svg]:size-3.5 [&_svg]:shrink-0 [&_svg]:text-faint"
    >
      {icon}
      <span className="min-w-0 flex-1 truncate">{label}</span>
      {hint ? <span className="shrink-0 text-[11.5px] text-faint">{hint}</span> : null}
    </Command.Item>
  )
}
