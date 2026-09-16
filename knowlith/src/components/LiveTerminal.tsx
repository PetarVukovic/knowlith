import { useEffect, useRef, useState } from "react"
import { Loader2, X } from "lucide-react"
import { Terminal } from "@xterm/xterm"
import { FitAddon } from "@xterm/addon-fit"
import "@xterm/xterm/css/xterm.css"
import { Button } from "@/components/ui/button"
import { terminalSocketUrl } from "@/lib/api"

/**
 * A real PTY session inside Knowlith — not a transcript of Terminal.app.
 *
 * Bytes come from `/api/terminal` (portable-pty on the daemon). The owner
 * types here; Claude Code / Codex / Cursor Agent run on this machine.
 */
export function LiveTerminal({
  app,
  prompt,
  label,
  onClose,
}: {
  app: string
  prompt: string
  label: string
  onClose: () => void
}) {
  const hostRef = useRef<HTMLDivElement>(null)
  const termRef = useRef<Terminal | null>(null)
  const fitRef = useRef<FitAddon | null>(null)
  const wsRef = useRef<WebSocket | null>(null)
  const [status, setStatus] = useState<"connecting" | "live" | "error" | "exit">("connecting")
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    const host = hostRef.current
    if (!host) return

    const theme = {
      background: "#0c0c0c",
      foreground: "#e8e8e8",
      cursor: "#e8e8e8",
      cursorAccent: "#0c0c0c",
      selectionBackground: "#264f78",
      // Full ANSI palette — without these, CLI colours collapse to grey.
      black: "#1e1e1e",
      red: "#f44747",
      green: "#6a9955",
      yellow: "#dcdcaa",
      blue: "#569cd6",
      magenta: "#c586c0",
      cyan: "#4ec9b0",
      white: "#d4d4d4",
      brightBlack: "#808080",
      brightRed: "#f14c4c",
      brightGreen: "#89d185",
      brightYellow: "#f5f543",
      brightBlue: "#6cb6ff",
      brightMagenta: "#d670d6",
      brightCyan: "#56d4c6",
      brightWhite: "#ffffff",
    }

    const term = new Terminal({
      cursorBlink: true,
      fontSize: 12.5,
      lineHeight: 1.2,
      fontFamily:
        "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, 'Liberation Mono', monospace",
      theme,
      allowProposedApi: true,
      convertEol: true,
    })
    const fit = new FitAddon()
    term.loadAddon(fit)
    term.open(host)
    fit.fit()
    termRef.current = term
    fitRef.current = fit

    const ws = new WebSocket(terminalSocketUrl())
    ws.binaryType = "arraybuffer"
    wsRef.current = ws

    ws.onopen = () => {
      ws.send(JSON.stringify({ type: "start", app, prompt }))
      const dims = { cols: term.cols, rows: term.rows }
      ws.send(JSON.stringify({ type: "resize", cols: dims.cols, rows: dims.rows }))
    }

    ws.onmessage = (ev) => {
      if (typeof ev.data === "string") {
        try {
          const msg = JSON.parse(ev.data) as { type: string; message?: string; label?: string }
          if (msg.type === "ready") {
            setStatus("live")
            return
          }
          if (msg.type === "error") {
            setStatus("error")
            setError(msg.message ?? "Terminal failed to start")
            term.writeln(`\r\n\x1b[31m${msg.message ?? "error"}\x1b[0m`)
            return
          }
          if (msg.type === "exit") {
            setStatus("exit")
            term.writeln("\r\n\x1b[90m[session ended]\x1b[0m")
          }
        } catch {
          term.write(ev.data)
        }
        return
      }
      term.write(new Uint8Array(ev.data as ArrayBuffer))
    }

    ws.onerror = () => {
      // Only surface this when the socket never came up — a mid-session
      // glitch must not paint "Knowlith is not running" over a live agent.
      setStatus((s) => {
        if (s === "connecting") {
          setError("Could not reach the live terminal. Is Knowlith running?")
          return "error"
        }
        return s
      })
    }
    ws.onclose = () => {
      setStatus((s) => (s === "live" ? "exit" : s))
    }

    term.onData((data) => {
      if (ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify({ type: "input", data }))
      }
    })

    const onResize = () => {
      fit.fit()
      if (ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify({ type: "resize", cols: term.cols, rows: term.rows }))
      }
    }
    const ro = new ResizeObserver(onResize)
    ro.observe(host)
    window.addEventListener("resize", onResize)

    return () => {
      ro.disconnect()
      window.removeEventListener("resize", onResize)
      ws.close()
      term.dispose()
      termRef.current = null
      fitRef.current = null
      wsRef.current = null
    }
  }, [app, prompt])

  return (
    <div className="flex h-full min-h-0 flex-col bg-[#0c0c0c] text-white">
      <div className="flex shrink-0 items-center justify-between gap-2 border-b border-white/10 px-2.5 py-1.5">
        <div className="min-w-0">
          <div className="truncate text-[12px] font-medium">{label}</div>
          <div className="text-[10.5px] text-white/45">
            {status === "connecting"
              ? "Starting…"
              : status === "live"
                ? "Live"
                : status === "exit"
                  ? "Ended"
                  : "Failed"}
          </div>
        </div>
        <Button
          variant="ghost"
          size="sm"
          className="size-7 shrink-0 p-0 text-white/60 hover:bg-white/10 hover:text-white"
          onClick={onClose}
          aria-label="Close terminal"
        >
          <X className="size-3.5" />
        </Button>
      </div>
      {status === "connecting" ? (
        <div className="flex items-center gap-2 px-2.5 py-1.5 text-[11.5px] text-white/50">
          <Loader2 className="size-3 animate-spin" />
          Connecting…
        </div>
      ) : null}
      {error ? <p className="px-2.5 py-1.5 text-[11.5px] text-red-300">{error}</p> : null}
      <div ref={hostRef} className="min-h-0 flex-1 px-0.5 pb-0.5" />
    </div>
  )
}
