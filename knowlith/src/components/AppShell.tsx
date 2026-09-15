import { useState } from "react"
import { Outlet } from "react-router-dom"
import * as DialogPrimitive from "@radix-ui/react-dialog"
import { CommandPalette } from "@/components/CommandPalette"
import { ResizeHandle, usePanelSize } from "@/components/Resizable"
import { Sidebar } from "@/components/Sidebar"
import { StatusBar } from "@/components/StatusBar"
import { TopBar } from "@/components/TopBar"
import { useApp } from "@/state/AppState"

export function AppShell() {
  const [navOpen, setNavOpen] = useState(false)
  const { ready } = useApp()
  const sidebar = usePanelSize("sidebar", 248, 190, 460)

  return (
    <div className="flex h-full min-h-0 flex-col bg-bg">
      <TopBar onOpenNav={() => setNavOpen(true)} />
      <div className="flex min-h-0 flex-1">
        <div className="hidden shrink-0 md:block" style={{ width: sidebar.width }}>
          <Sidebar />
        </div>
        <ResizeHandle panel={sidebar} edge="start" label="Resize navigation" className="hidden md:block" />
        <main className="scroll-thin min-w-0 flex-1 overflow-y-auto">
          {ready ? <Outlet /> : <LoadingPane />}
        </main>
      </div>
      <StatusBar />
      <CommandPalette />

      <DialogPrimitive.Root open={navOpen} onOpenChange={setNavOpen}>
        <DialogPrimitive.Portal>
          <DialogPrimitive.Overlay className="fixed inset-0 z-50 bg-black/30 md:hidden" />
          <DialogPrimitive.Content
            className="fixed inset-y-0 left-0 z-50 w-[248px] border-r border-line outline-none md:hidden"
            onClick={() => setNavOpen(false)}
          >
            <DialogPrimitive.Title className="sr-only">Navigation</DialogPrimitive.Title>
            <Sidebar />
          </DialogPrimitive.Content>
        </DialogPrimitive.Portal>
      </DialogPrimitive.Root>
    </div>
  )
}

/** The daemon answers in milliseconds locally; this is a flicker guard, not a spinner screen. */
function LoadingPane() {
  return (
    <div className="grid h-full place-items-center">
      <span className="text-[12.5px] text-faint">Reading your context…</span>
    </div>
  )
}
