import { useEffect, useState } from "react"
import { AppWindow, Loader2, Terminal } from "lucide-react"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"
import { tools as toolsApi, failed } from "@/lib/api"
import type { AiTool } from "@/lib/types"
import { cn } from "@/lib/utils"

export type AskPickResult = {
  label: string
  surface: "desktop" | "terminal" | "missing"
  command: string | null
  message: string
  slug: string
}

/**
 * Asks which connected AI should answer, then opens it outside Knowlith
 * with the question ready in the composer or a Terminal session.
 */
export function AskAiPicker({
  open,
  onOpenChange,
  prompt,
  about,
  onLaunched,
  onNeedsConnect,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  prompt: string
  about?: string
  onLaunched: (result: AskPickResult) => void
  onNeedsConnect: () => void
}) {
  const [tools, setTools] = useState<AiTool[] | null>(null)
  const [busy, setBusy] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!open) return
    setError(null)
    setBusy(null)
    let cancelled = false
    void toolsApi.list().then((list) => {
      if (!cancelled) setTools(list)
    })
    return () => {
      cancelled = true
    }
  }, [open])

  const installed = (tools ?? []).filter((t) => t.installed && t.launchSurface !== "missing")

  const pick = async (tool: AiTool) => {
    setBusy(tool.slug)
    setError(null)
    const result = await toolsApi.try(tool.slug, prompt)
    setBusy(null)
    if (failed(result)) {
      setError(result.error)
      return
    }
    onLaunched({
      label: result.label,
      surface: result.surface,
      command: result.command,
      message: result.message,
      slug: result.app,
    })
    onOpenChange(false)
  }

  const describe = (tool: AiTool): string => {
    if (tool.launchSurface === "terminal") {
      const bin = tool.slug === "cursor" ? "agent" : tool.slug === "claude-code" ? "claude" : "codex"
      return `Opens Terminal with \`${bin}\` and the question ready`
    }
    return "Opens the desktop app with the question ready"
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-[420px]">
        <DialogTitle>Which AI should answer?</DialogTitle>
        <DialogDescription>
          {about ? `About “${about}”. ` : ""}
          Desktop apps open outside Knowlith; CLIs open in Terminal.
        </DialogDescription>

        <p className="mt-3 rounded-md bg-surface-2 px-3 py-2 text-[12.5px] italic text-muted">
          “{prompt.length > 160 ? `${prompt.slice(0, 159)}…` : prompt}”
        </p>

        {tools === null ? (
          <p className="mt-4 flex items-center gap-2 text-[13px] text-faint">
            <Loader2 className="size-3.5 animate-spin" />
            Checking what is connected…
          </p>
        ) : installed.length === 0 ? (
          <div className="mt-4 grid gap-3">
            <p className="text-[13px] text-muted">No AI assistant is installed on this Mac.</p>
            <Button
              variant="primary"
              onClick={() => {
                onOpenChange(false)
                onNeedsConnect()
              }}
            >
              See AI assistants
            </Button>
          </div>
        ) : (
          <ul className="mt-4 grid gap-2">
            {installed.map((tool) => {
              const Icon = tool.launchSurface === "terminal" ? Terminal : AppWindow
              const ready = tool.connected
              return (
                <li key={tool.slug}>
                  <button
                    type="button"
                    disabled={busy !== null}
                    onClick={() => {
                      if (!ready) {
                        onOpenChange(false)
                        onNeedsConnect()
                        return
                      }
                      void pick(tool)
                    }}
                    className={cn(
                      "flex w-full items-center gap-3 rounded-lg border border-line bg-surface px-3 py-3 text-left transition-colors",
                      ready && "hover:border-accent/40 hover:bg-accent-soft/40",
                      !ready && "opacity-90",
                      busy === tool.slug && "opacity-70",
                    )}
                  >
                    <span className="grid size-9 shrink-0 place-items-center rounded-md bg-surface-2 text-muted">
                      <Icon className="size-4" />
                    </span>
                    <span className="min-w-0 flex-1">
                      <span className="block text-[13.5px] font-medium text-ink">{tool.label}</span>
                      <span className="block text-[12px] text-muted">
                        {ready ? describe(tool) : "Connect MCP in AI assistants first"}
                      </span>
                    </span>
                    {busy === tool.slug ? (
                      <Loader2 className="size-4 shrink-0 animate-spin text-faint" />
                    ) : null}
                  </button>
                </li>
              )
            })}
          </ul>
        )}

        {error ? <p className="mt-3 text-[12.5px] text-conflict">{error}</p> : null}
      </DialogContent>
    </Dialog>
  )
}
