import { Check, CircleCheck, Cloud, Terminal } from "lucide-react"
import type { Processor } from "@/lib/types"
import { cn } from "@/lib/utils"

/**
 * Who reads the documents.
 *
 * Each option states plainly what leaves the disk, because that is the only
 * difference between them that the owner should have to weigh. Two of the
 * three keep the reading on the machine under an account they already pay
 * for; one does not, and says so in the same size type as everything else.
 */
const OPTIONS: {
  id: Processor
  title: string
  subtitle: string
  Icon: typeof Terminal
  leaves: string
  cost: string
  detected: boolean
}[] = [
  {
    id: "codex",
    title: "Use Codex on this Mac",
    subtitle: "Reading happens here, under your own Codex account.",
    Icon: Terminal,
    leaves: "Only short quotes leave this Mac, and only to OpenAI under your account.",
    cost: "No extra cost from us.",
    detected: true,
  },
  {
    id: "claude-code",
    title: "Use Claude Code",
    subtitle: "Reading happens here, under your own Claude account.",
    Icon: Terminal,
    leaves: "Only short quotes leave this Mac, and only to Anthropic under your account.",
    cost: "No extra cost from us.",
    detected: true,
  },
  {
    id: "managed",
    title: "Use Knowlith Managed AI",
    subtitle: "We do the reading for you. Nothing to install.",
    Icon: Cloud,
    leaves: "Document text is sent to our service while it is being read.",
    cost: "Included in your plan.",
    detected: false,
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
  const selected = OPTIONS.find((o) => o.id === value)!
  const local = value !== "managed"

  return (
    <div>
      <h1 className="text-[26px] font-semibold leading-tight tracking-[-0.022em] text-ink">Choose who reads them</h1>
      <p className="mt-2.5 max-w-[48ch] text-[14px] leading-relaxed text-muted">
        Knowlith found two AI tools already installed on this Mac. Using one of them keeps the reading on your
        machine and on your existing subscription.
      </p>

      <div className="mt-8 grid gap-2.5">
        {OPTIONS.map((option) => {
          const active = value === option.id
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
                  {option.detected ? (
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
            Allow Knowlith to start {selected.title.replace("Use ", "").replace(" on this Mac", "")} for approved
            analysis jobs.
            <span className="mt-1 block text-[12px] text-muted">
              It runs only while reading your files, and stops when the work is done.
            </span>
          </span>
        </label>
      ) : null}
    </div>
  )
}
