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
  embedded: boolean
}

/**
 * Asks which connected AI to open, then launches that one for real.
 *
 * With `embedded`, CLI hosts prepare a live in-app PTY (no Terminal.app).
 * Desktop apps still open outside.
 */
export function AskAiPicker({
  open,
  onOpenChange,
  prompt,
  about,
  embedded = false,
  onLaunched,
  onNeedsConnect,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  prompt: string
  about?: string
  /** Prefer the in-app live terminal for CLI assistants (Company brain). */
  embedded?: boolean
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

  const choices = (tools ?? []).filter((t) => t.connected && t.launchSurface !== "missing")

  /** CLI hosts the brain can keep beside the map (not Claude Desktop). */
  const canEmbedLive = (tool: AiTool) =>
    tool.launchSurface === "terminal" ||
    tool.slug === "claude-code" ||
    tool.slug === "codex" ||
    tool.slug === "cursor"

  const pick = async (tool: AiTool) => {
    setBusy(tool.slug)
    setError(null)
    // Always ask the daemon for an in-app PTY when the brain embeds; the
    // server falls back to a desktop deep link when there is no CLI.
    const wantEmbedded = embedded && canEmbedLive(tool)
    const result = await toolsApi.try(tool.slug, prompt, { embedded: wantEmbedded })
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
      // If an older daemon omits the field, still open LiveTerminal when we
      // requested an embed and the surface is a CLI session.
      embedded: result.embedded === true || (wantEmbedded && result.surface === "terminal"),
    })
    onOpenChange(false)
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-[420px]">
        <DialogTitle>Which AI should answer?</DialogTitle>
        <DialogDescription>
          {embedded
            ? about
              ? `About “${about}”. CLI assistants open live beside the map; desktop apps open outside.`
              : "CLI assistants open live beside the map; desktop apps open outside."
            : about
              ? `About “${about}”. Pick an assistant — CLIs open Terminal; desktop apps open outside.`
              : "Pick an assistant. CLIs open a real Terminal window; desktop apps open outside Knowlith."}
        </DialogDescription>

        <p className="mt-3 rounded-md bg-surface-2 px-3 py-2 text-[12.5px] italic text-muted">
          “{prompt.length > 160 ? `${prompt.slice(0, 159)}…` : prompt}”
        </p>

        {tools === null ? (
          <p className="mt-4 flex items-center gap-2 text-[13px] text-faint">
            <Loader2 className="size-3.5 animate-spin" />
            Checking what is connected…
          </p>
        ) : choices.length === 0 ? (
          <div className="mt-4 grid gap-3">
            <p className="text-[13px] text-muted">No AI assistant is connected yet.</p>
            <Button
              variant="primary"
              onClick={() => {
                onOpenChange(false)
                onNeedsConnect()
              }}
            >
              Connect an assistant
            </Button>
          </div>
        ) : (
          <ul className="mt-4 grid gap-2">
            {choices.map((tool) => {
              const live = embedded && canEmbedLive(tool)
              const terminal = tool.launchSurface === "terminal" || live
              return (
                <li key={tool.slug}>
                  <button
                    type="button"
                    disabled={busy !== null}
                    onClick={() => void pick(tool)}
                    className={cn(
                      "flex w-full items-center gap-3 rounded-lg border border-line bg-surface px-3 py-3 text-left transition-colors",
                      "hover:border-accent/40 hover:bg-accent-soft/40",
                      busy === tool.slug && "opacity-70",
                    )}
                  >
                    <span className="grid size-9 shrink-0 place-items-center rounded-md bg-surface-2 text-muted">
                      {terminal ? <Terminal className="size-4" /> : <AppWindow className="size-4" />}
                    </span>
                    <span className="min-w-0 flex-1">
                      <span className="block text-[13.5px] font-medium text-ink">{tool.label}</span>
                      <span className="block text-[12px] text-muted">
                        {live
                          ? tool.slug === "cursor"
                            ? "Live `agent` session beside the map"
                            : tool.slug === "claude-code"
                              ? "Live `claude` session beside the map"
                              : tool.slug === "codex"
                                ? "Live `codex` session beside the map"
                                : "Live terminal beside the map"
                          : tool.slug === "cursor" && tool.launchSurface === "terminal"
                            ? "Runs `agent` in a real Terminal window (Cursor Agent CLI)"
                            : tool.slug === "claude-code"
                              ? "Runs `claude` in a real Terminal window"
                              : tool.slug === "codex" && tool.launchSurface === "terminal"
                                ? "Runs `codex` in a real Terminal window"
                                : tool.launchSurface === "terminal"
                                  ? "Opens a real Terminal window with the question"
                                  : "Opens the desktop app with the question ready"}
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
