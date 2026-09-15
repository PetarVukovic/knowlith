import { useState } from "react"
import { useNavigate } from "react-router-dom"
import { ArrowRight, Check, Code2, Copy, MessageSquare, Plug, Sparkles, Terminal } from "lucide-react"
import { Button } from "@/components/ui/button"
import { useApp } from "@/state/AppState"

/**
 * Connecting the tools the company already uses.
 *
 * This screen comes last on purpose. Explaining how to wire an AI tool into a
 * knowledge base is a hard sell before the owner has one; after they have
 * approved their first rule it is obvious, and the instruction is one click.
 */
// Four of these start with the same letter, so an initial tells the owner
// nothing. An icon for what the tool *is* does.
const TOOLS = [
  { id: "claude", name: "Claude", detail: "Desktop app and Claude Code.", Icon: Sparkles, ready: true },
  { id: "codex", name: "Codex", detail: "The CLI on this Mac.", Icon: Terminal, ready: true },
  { id: "cursor", name: "Cursor", detail: "Editor and agent mode.", Icon: Code2, ready: true },
  { id: "chatgpt", name: "ChatGPT", detail: "Desktop app.", Icon: MessageSquare, ready: false },
]

const CONFIG = `{
  "mcpServers": {
    "knowlith": {
      "command": "knowlithd",
      "args": ["serve", "--stdio"]
    }
  }
}`

export function Connect() {
  const navigate = useNavigate()
  const { firstRun, setFirstRun, companyName } = useApp()
  const [connected, setConnected] = useState<string[]>([])
  const [copied, setCopied] = useState(false)

  const copyConfig = async () => {
    try {
      await navigator.clipboard.writeText(CONFIG)
      setCopied(true)
      window.setTimeout(() => setCopied(false), 2000)
    } catch {
      /* clipboard blocked — the configuration is visible below either way */
    }
  }

  const finish = () => {
    setFirstRun(null)
    navigate("/home")
  }

  return (
    <div className="mx-auto w-full max-w-[720px] px-5 py-10">
      <h1 className="text-[24px] font-semibold leading-tight tracking-[-0.022em] text-ink">
        Use your company context anywhere
      </h1>
      <p className="mt-2.5 max-w-[52ch] text-[14px] leading-relaxed text-muted">
        Connect the tools your team already uses. From then on they answer from what {companyName} approved,
        and they cite the document it came from.
      </p>

      <div className="mt-8 grid gap-2.5 sm:grid-cols-2">
        {TOOLS.map((tool) => {
          const isConnected = connected.includes(tool.id)
          return (
            <div
              key={tool.id}
              className="flex items-center gap-3 rounded-xl border border-line bg-surface p-4"
            >
              <span className="grid size-9 shrink-0 place-items-center rounded-lg bg-surface-2 text-muted">
                <tool.Icon className="size-4" />
              </span>
              <span className="min-w-0 flex-1">
                <span className="block text-[14px] font-medium text-ink">{tool.name}</span>
                <span className="block text-[12.5px] text-muted">{tool.detail}</span>
              </span>
              {tool.ready ? (
                <Button
                  size="sm"
                  variant={isConnected ? "subtle" : "default"}
                  onClick={() => setConnected((c) => (c.includes(tool.id) ? c : [...c, tool.id]))}
                >
                  {isConnected ? <Check /> : <Plug />}
                  {isConnected ? "Connected" : "Connect"}
                </Button>
              ) : (
                <span className="shrink-0 text-[12px] text-faint">Coming soon</span>
              )}
            </div>
          )
        })}
      </div>

      <div className="mt-7 rounded-xl border border-line bg-surface-2 p-4">
        <div className="flex flex-wrap items-center gap-3">
          <span className="text-[13px] font-medium text-ink">Another tool?</span>
          <span className="flex-1 text-[12.5px] text-muted">
            Paste this into its configuration file.
          </span>
          <Button size="sm" variant="default" onClick={() => void copyConfig()}>
            {copied ? <Check /> : <Copy />}
            {copied ? "Copied" : "Copy configuration"}
          </Button>
        </div>
        <pre className="scroll-thin mt-3 overflow-x-auto rounded-lg border border-line bg-surface p-3 font-mono text-[11.5px] leading-relaxed text-muted">
          {CONFIG}
        </pre>
      </div>

      {firstRun === "connect" ? (
        <div className="mt-9 flex flex-wrap items-center gap-3">
          <Button size="lg" variant="primary" onClick={finish}>
            Done
            <ArrowRight />
          </Button>
          <button
            type="button"
            onClick={finish}
            className="text-[12px] text-faint underline-offset-4 transition-colors hover:text-muted hover:underline"
          >
            I'll connect a tool later
          </button>
        </div>
      ) : null}
    </div>
  )
}
