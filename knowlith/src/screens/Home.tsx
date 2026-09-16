import { useEffect, useState } from "react"
import { useNavigate } from "react-router-dom"
import { ArrowRight, CheckCircle2, GitMerge, PauseCircle, Sparkles } from "lucide-react"
import { AskAiPicker } from "@/components/AskAiPicker"
import { LiveWork } from "@/components/LiveWork"
import { Tooltip } from "@/components/ui/tooltip"
import { Button } from "@/components/ui/button"
import { Panel } from "@/components/ui/surface"
import { tools as toolsApi } from "@/lib/api"
import { companyKnowledgePrompt } from "@/lib/askAi"
import type { AiTool, ObjectKind } from "@/lib/types"
import { cn, formatRelative } from "@/lib/utils"
import { useApp } from "@/state/AppState"

const COUNT_HELP = {
  rule: {
    label: "Rules",
    hint: "A decision your company already made — limits, deadlines, who approves what.",
    kind: "rule" as ObjectKind | "skill",
  },
  process: {
    label: "Processes",
    hint: "How work is done here, step by step.",
    kind: "process" as ObjectKind | "skill",
  },
  skill: {
    label: "AI skills",
    hint: "Tasks connected AI assistants can carry out using your approved knowledge.",
    kind: "skill" as ObjectKind | "skill",
  },
  term: {
    label: "Business terms",
    hint: "Words, products and meanings specific to your company.",
    kind: "term" as ObjectKind | "skill",
  },
} as const

function greeting(name: string): string {
  const hour = new Date().getHours()
  const first = name.trim().split(/\s+/)[0] || "there"
  if (hour < 12) return `Good morning, ${first}`
  if (hour < 18) return `Good afternoon, ${first}`
  return `Good evening, ${first}`
}

/**
 * Home answers three questions in five seconds: what we know, what changed,
 * and whether the owner must act. Compiler telemetry stays in Engineer mode.
 */
export function Home() {
  const { companyName, objects, skills, review, sources, mode, work } = useApp()
  const navigate = useNavigate()
  const [assistants, setAssistants] = useState<AiTool[] | null>(null)
  const [askHint, setAskHint] = useState<string | null>(null)
  const [pickerOpen, setPickerOpen] = useState(false)

  useEffect(() => {
    let cancelled = false
    void toolsApi.list().then((list) => {
      if (!cancelled) setAssistants(list)
    })
    return () => {
      cancelled = true
    }
  }, [])

  const knowledgePrompt = companyKnowledgePrompt(companyName)

  const approved = objects.filter((o) => o.status === "approved")
  const counts = {
    rule: approved.filter((o) => o.kind === "rule").length,
    process: approved.filter((o) => o.kind === "process").length,
    skill: skills.filter((s) => s.status === "approved").length,
    term: approved.filter((o) => o.kind === "term" || o.kind === "fact").length,
  }

  const conflicts = review.filter((r) => r.conflict).length
  const newRules = review.filter((r) => !r.conflict && r.kind === "rule").length
  const newProcesses = review.filter((r) => !r.conflict && r.kind === "process").length
  const otherWaiting = review.length - conflicts - newRules - newProcesses
  const paused = sources.filter((s) => s.status === "paused").length

  const attentionLines: string[] = []
  if (newRules > 0) {
    attentionLines.push(
      `${newRules} new ${newRules === 1 ? "rule" : "rules"} to confirm`,
    )
  }
  if (newProcesses > 0) {
    attentionLines.push(
      `${newProcesses} new ${newProcesses === 1 ? "process" : "processes"} found`,
    )
  }
  if (otherWaiting > 0) {
    attentionLines.push(
      `${otherWaiting} other ${otherWaiting === 1 ? "item" : "items"} waiting for confirmation`,
    )
  }
  if (conflicts > 0) {
    attentionLines.push(
      `${conflicts} ${conflicts === 1 ? "disagreement" : "disagreements"} between documents`,
    )
  }
  if (paused > 0) {
    attentionLines.push(
      `${paused} ${paused === 1 ? "source is" : "sources are"} paused`,
    )
  }

  const nothingRead = objects.length === 0
  const noFolders = sources.length === 0
  const needsYou = attentionLines.length > 0
  const healthy = !nothingRead && !needsYou

  const statusLine = noFolders
    ? "Add a source so Knowlith can read your business files."
    : work.queued + work.working > 0
      ? "Knowlith is reviewing your business files…"
      : nothingRead
        ? "Your sources are connected. Nothing has come out of them yet."
        : healthy
          ? "Knowlith has reviewed your business files. Everything is up to date."
          : "Knowlith found changes that need your confirmation before AI assistants can use them."

  const connected = (assistants ?? []).filter((t) => t.connected)

  return (
    <div className="mx-auto w-full max-w-[720px] px-4 py-8">
      <h1 className="text-[22px] font-semibold tracking-[-0.02em] text-ink">
        {greeting(companyName === "Your company" ? "" : companyName)}
      </h1>
      <p
        className={cn(
          "mt-2 flex items-start gap-1.5 text-[14px] leading-relaxed",
          healthy ? "text-confirmed" : "text-muted",
        )}
      >
        {healthy ? <CheckCircle2 className="mt-0.5 size-4 shrink-0" /> : null}
        <span>{statusLine}</span>
      </p>

      <div className="mt-5 flex flex-wrap items-center gap-2">
        <Button
          variant="primary"
          data-tour="tour-ask-ai"
          onClick={() => {
            setAskHint(null)
            setPickerOpen(true)
          }}
        >
          <Sparkles className="size-3.5" />
          Ask AI how well it knows {companyName}
        </Button>
        <Button variant="default" onClick={() => navigate("/brain")}>
          Company brain
        </Button>
        <Button
          variant="ghost"
          size="sm"
          onClick={() => window.dispatchEvent(new Event("knowlith:start-tour"))}
        >
          Show me around
        </Button>
      </div>
      {askHint ? <p className="mt-2 text-[12.5px] text-muted">{askHint}</p> : null}

      {mode === "engineer" ? (
        <div className="mt-6 empty:mt-0">
          <LiveWork />
        </div>
      ) : null}

      <section className="mt-8">
        <h2 className="text-[15px] font-semibold text-ink">Needs your attention</h2>
        {attentionLines.length === 0 ? (
          <Panel className="mt-3 p-4 text-[13px] text-muted">
            {nothingRead
              ? "Nothing to decide yet. Findings appear here as they come out of your sources."
              : "No action needed. New findings appear here the next time Knowlith reads your sources."}
          </Panel>
        ) : (
          <Panel className="mt-3 p-4">
            <ul className="grid gap-1.5 text-[13.5px] text-ink">
              {attentionLines.map((line) => (
                <li key={line} className="flex items-start gap-2">
                  {line.includes("disagreement") ? (
                    <GitMerge className="mt-0.5 size-3.5 shrink-0 text-conflict" />
                  ) : line.includes("paused") ? (
                    <PauseCircle className="mt-0.5 size-3.5 shrink-0 text-pending" />
                  ) : (
                    <span className="mt-1.5 size-1.5 shrink-0 rounded-full bg-pending" />
                  )}
                  {line}
                </li>
              ))}
            </ul>
            <Button className="mt-4" variant="primary" onClick={() => navigate("/review")}>
              Review what changed
              <ArrowRight />
            </Button>
          </Panel>
        )}
      </section>

      <section className="mt-9">
        <h2 className="text-[15px] font-semibold text-ink">Your company</h2>
        <div className="mt-3 grid grid-cols-2 gap-px overflow-hidden rounded-lg border border-line bg-line sm:grid-cols-4">
          {(Object.keys(COUNT_HELP) as Array<keyof typeof COUNT_HELP>).map((key) => {
            const meta = COUNT_HELP[key]
            return (
              <Tooltip key={key} content={meta.hint}>
                <button
                  type="button"
                  onClick={() =>
                    navigate(
                      meta.kind === "term"
                        ? "/browse?kind=fact"
                        : meta.kind === "skill"
                          ? "/browse?kind=skill"
                          : `/browse?kind=${meta.kind}`,
                    )
                  }
                  className="bg-surface px-4 py-3.5 text-left transition-colors hover:bg-surface-2"
                >
                  <div className="tabular text-[26px] font-semibold leading-none text-ink">
                    {counts[key]}
                  </div>
                  <div className="mt-1.5 text-[12px] text-muted">{meta.label}</div>
                </button>
              </Tooltip>
            )
          })}
        </div>
      </section>

      <section className="mt-9">
        <div className="flex items-baseline justify-between gap-3">
          <h2 className="text-[15px] font-semibold text-ink">AI assistants</h2>
          <button
            type="button"
            onClick={() => navigate("/connect")}
            className="text-[12.5px] text-muted hover:text-ink"
          >
            Manage
          </button>
        </div>
        {assistants === null ? (
          <p className="mt-3 text-[13px] text-faint">Checking what is installed…</p>
        ) : assistants.length === 0 ? (
          <Panel className="mt-3 p-4 text-[13px] text-muted">
            No assistants found on this computer yet.
          </Panel>
        ) : (
          <ul className="mt-3 grid gap-px overflow-hidden rounded-lg border border-line bg-line">
            {assistants.map((tool) => (
              <li
                key={tool.slug}
                className="flex flex-wrap items-center justify-between gap-2 bg-surface px-3.5 py-2.5"
              >
                <span className="text-[13.5px] font-medium text-ink">{tool.label}</span>
                <span className="flex items-center gap-3 text-[12.5px]">
                  {tool.connected ? (
                    <span className="text-confirmed">
                      Connected
                      {tool.lastRead ? (
                        <span className="text-faint"> · used {formatRelative(tool.lastRead)}</span>
                      ) : tool.reads === 0 ? (
                        <span className="text-faint"> · not used yet</span>
                      ) : null}
                    </span>
                  ) : tool.installed ? (
                    <button
                      type="button"
                      onClick={() => navigate("/connect")}
                      className="text-accent hover:underline"
                    >
                      Connect
                    </button>
                  ) : (
                    <span className="text-faint">Not installed</span>
                  )}
                </span>
              </li>
            ))}
            {connected.length === 0 && assistants.some((t) => t.installed) ? (
              <li className="bg-surface px-3.5 py-2 text-[12.5px] text-muted">
                Connect an assistant so it can answer from what {companyName} approved.
              </li>
            ) : null}
          </ul>
        )}
      </section>

      <AskAiPicker
        open={pickerOpen}
        onOpenChange={setPickerOpen}
        prompt={knowledgePrompt}
        about={companyName}
        onNeedsConnect={() => navigate("/connect")}
        onLaunched={(result) => setAskHint(result.message)}
      />
    </div>
  )
}
