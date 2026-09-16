import { Terminal, X } from "lucide-react"
import { Button } from "@/components/ui/button"

/**
 * Shows the Terminal session Knowlith just opened for a CLI assistant.
 *
 * This is not a fake PTY pretending to be the agent — the real session is
 * in Terminal.app / the system console. The panel names what was opened and
 * keeps the command visible so the owner can re-run it if the window failed.
 */
export function TerminalSession({
  label,
  command,
  message,
  onClose,
}: {
  label: string
  command: string | null
  message: string
  onClose: () => void
}) {
  return (
    <div className="fixed inset-x-0 bottom-0 z-40 border-t border-line bg-ink text-[12.5px] text-white shadow-lg">
      <div className="mx-auto flex max-w-[1100px] items-start gap-3 px-4 py-3">
        <Terminal className="mt-0.5 size-4 shrink-0 text-white/70" />
        <div className="min-w-0 flex-1">
          <div className="font-medium text-white">{label} · Terminal session</div>
          <p className="mt-0.5 text-white/70">{message}</p>
          {command ? (
            <pre className="mt-2 overflow-x-auto rounded bg-black/40 px-2.5 py-2 font-mono text-[11.5px] text-white/90">
              {command}
            </pre>
          ) : null}
        </div>
        <Button
          variant="ghost"
          size="sm"
          className="shrink-0 text-white/70 hover:bg-white/10 hover:text-white"
          onClick={onClose}
          aria-label="Dismiss"
        >
          <X className="size-3.5" />
        </Button>
      </div>
    </div>
  )
}
