import { useEffect, useState } from "react"
import { Check, CircleCheck, Cloud, Terminal, Wrench } from "lucide-react"
import { background } from "@/lib/api"
import type { DetectedEngine, Processor } from "@/lib/types"
import { cn } from "@/lib/utils"

/**
 * Who reads the documents.
 *
 * Each option states plainly what leaves the disk, because that is the only
 * difference between them that the owner should have to weigh. Local CLIs
 * keep the reading on the machine under an account they already pay for;
 * Managed does not, and says so in the same size type as everything else.
 */
const OPTIONS: {
  id: Processor
  title: string
  subtitle: string
  Icon: typeof Terminal
  leaves: string
  cost: string
}[] = [
  {
    id: "codex",
    title: "Use Codex on this Mac",
    subtitle: "Reading happens here, under your own Codex account.",
    Icon: Terminal,
    leaves: "Only short quotes leave this Mac, and only to OpenAI under your account.",
    cost: "No extra cost from us.",
  },
  {
    id: "claude-code",
    title: "Use Claude Code",
    subtitle: "Reading happens here, under your own Claude account.",
    Icon: Terminal,
    leaves: "Only short quotes leave this Mac, and only to Anthropic under your account.",
    cost: "No extra cost from us.",
  },
  {
    id: "cursor-agent",
    title: "Use Cursor Agent",
    subtitle: "Reading happens here, under your own Cursor account (`agent` CLI).",
    Icon: Wrench,
    leaves: "Only short quotes leave this Mac, and only to Cursor under your account.",
    cost: "No extra cost from us.",
  },
  {
    id: "managed",
    title: "Use Knowlith Managed AI",
    subtitle: "We do the reading for you. Nothing to install.",
    Icon: Cloud,
    leaves: "Document text is sent to our service while it is being read.",
    cost: "Included in your plan.",
  },
]

export function StepProcessing({
  value,
  onChange,
  allowStart,
  onAllowStart,
}: {
  value: Processor
  onChange: (p: Processor) => void
  allowStart: boolean
  onAllowStart: (v: boolean) => void
}) {
  const [engines, setEngines] = useState<DetectedEngine[] | null>(null)

  useEffect(() => {
    let cancelled = false
    void background.engines().then((list) => {
      if (!cancelled) setEngines(list)
    })
    return () => {
      cancelled = true
    }
  }, [])

  const selected = OPTIONS.find((o) => o.id === value)!
  const local = value !== "managed"
  const installedCount = (engines ?? []).filter((e) => e.installed && e.id !== "managed").length

  const isDetected = (id: Processor) => {
    if (id === "managed") return false
    const hit = engines?.find((e) => e.id === id)
    return hit?.installed ?? false
  }

  return (
    <div>
      <h1 className="text-[26px] font-semibold leading-tight tracking-[-0.022em] text-ink">
        Choose who reads them
      </h1>
      <p className="mt-2.5 max-w-[48ch] text-[14px] leading-relaxed text-muted">
        {engines === null
          ? "Checking which AI tools are on this Mac…"
          : installedCount > 0
            ? `Knowlith found ${installedCount} AI tool${installedCount === 1 ? "" : "s"} already installed on this Mac. Using one of them keeps the reading on your machine and on your existing subscription.`
            : "No local AI command line was found yet. Install Claude Code, Codex or Cursor Agent, or choose Managed."}
      </p>

      <div className="mt-8 grid gap-2.5">
        {OPTIONS.map((option) => {
          const active = value === option.id
          const detected = isDetected(option.id)
          return (
            <button
              key={option.id}
              type="button"
              onClick={() => onChange(option.id)}
              className={cn(
                "flex gap-3.5 rounded-xl border p-4 text-left transition-all",
                active ? "border-accent bg-accent-soft" : "border-line bg-surface hover:border-line-strong",
              )}
            >
              <span
                className={cn(
                  "mt-0.5 grid size-[18px] shrink-0 place-items-center rounded-full border transition-colors",
                  active ? "border-accent bg-accent" : "border-line-strong",
                )}
              >
                {active ? <Check className="size-3 text-on-accent" /> : null}
              </span>
              <span className="min-w-0 flex-1">
                <span className="flex flex-wrap items-center gap-x-2 gap-y-1">
                  <option.Icon className="size-4 shrink-0 text-faint" />
                  <span className="text-[14px] font-medium text-ink">{option.title}</span>
                  {detected ? (
                    <span className="flex items-center gap-1 text-[11.5px] text-confirmed">
                      <CircleCheck className="size-3" />
                      Detected
                    </span>
                  ) : null}
                </span>
                <span className="mt-1 block text-[13px] text-muted">{option.subtitle}</span>
                <span className="mt-2.5 block border-t border-line pt-2.5 text-[12.5px] leading-relaxed text-muted">
                  {option.leaves} {option.cost}
                </span>
              </span>
            </button>
          )
        })}
      </div>

      {local ? (
        <label className="mt-5 flex cursor-pointer items-start gap-3 rounded-xl border border-line bg-surface-2 p-4 transition-colors hover:border-line-strong">
          <input
            type="checkbox"
            checked={allowStart}
            onChange={(e) => onAllowStart(e.target.checked)}
            className="mt-0.5 size-[18px] shrink-0 accent-[var(--k-accent)]"
          />
          <span className="text-[13px] leading-relaxed text-ink">
            Allow Knowlith to start {selected.title.replace("Use ", "").replace(" on this Mac", "")}{" "}
            for approved analysis jobs.
            <span className="mt-1 block text-[12px] text-muted">
              It runs only while reading your files, and stops when the work is done.
            </span>
          </span>
        </label>
      ) : null}
    </div>
  )
}
