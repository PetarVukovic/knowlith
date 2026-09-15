import { Laptop, Menu, Moon, Search, Sun } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Kbd } from "@/components/ui/surface"
import { Switch } from "@/components/ui/switch"
import { Tooltip } from "@/components/ui/tooltip"
import { useApp } from "@/state/AppState"

export function TopBar({ onOpenNav }: { onOpenNav: () => void }) {
  const { setPaletteOpen, mode, setMode, theme, setTheme, companyName } = useApp()

  return (
    <header className="flex h-12 shrink-0 items-center gap-2 border-b border-line bg-surface px-3">
      <Button variant="ghost" size="icon-sm" className="md:hidden" onClick={onOpenNav} aria-label="Open navigation">
        <Menu />
      </Button>

      <div className="hidden items-baseline gap-1.5 sm:flex">
        <span className="text-[13px] font-semibold text-ink">Knowlith</span>
        <span className="text-faint">/</span>
        <span className="truncate text-[13px] text-muted">{companyName}</span>
      </div>

      <button
        type="button"
        onClick={() => setPaletteOpen(true)}
        className="ml-auto flex h-8 min-w-0 max-w-[420px] flex-1 items-center gap-2 rounded-md border border-line bg-surface-2 px-2.5 text-left text-[12.5px] text-faint hover:border-line-strong sm:ml-4"
      >
        <Search className="size-3.5 shrink-0" />
        <span className="truncate">Search rules, processes, terms…</span>
        <span className="ml-auto hidden shrink-0 items-center gap-0.5 sm:flex">
          <Kbd>⌘</Kbd>
          <Kbd>K</Kbd>
        </span>
      </button>

      <div className="ml-auto flex shrink-0 items-center gap-2 sm:ml-4">
        <Tooltip content="Engineer mode shows the exact stored text, where in each file it came from, and what depends on what.">
          <label className="hidden cursor-pointer items-center gap-2 lg:flex">
            <span className="text-[12px] text-muted">Engineer mode</span>
            <Switch
              checked={mode === "engineer"}
              onCheckedChange={(checked) => setMode(checked ? "engineer" : "simple")}
              aria-label="Engineer mode"
            />
          </label>
        </Tooltip>

        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" size="icon-sm" aria-label="Appearance">
              {theme === "dark" ? <Moon /> : theme === "light" ? <Sun /> : <Laptop />}
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent>
            <DropdownMenuLabel>Appearance</DropdownMenuLabel>
            <DropdownMenuItem onSelect={() => setTheme("light")}>
              <Sun /> Light
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={() => setTheme("dark")}>
              <Moon /> Dark
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={() => setTheme("system")}>
              <Laptop /> Match system
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuLabel>Detail</DropdownMenuLabel>
            <DropdownMenuItem onSelect={() => setMode("simple")}>Simple mode</DropdownMenuItem>
            <DropdownMenuItem onSelect={() => setMode("engineer")}>Engineer mode</DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>

        <div className="grid size-7 shrink-0 place-items-center rounded-full border border-line bg-surface-3 text-[11px] font-semibold text-muted">
          AK
        </div>
      </div>
    </header>
  )
}
