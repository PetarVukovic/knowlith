import { useEffect, useState } from "react"
import { AlertTriangle, Check, Eye, EyeOff } from "lucide-react"
import { Panel, PanelHeader } from "@/components/ui/surface"
import { tools as toolsApi } from "@/lib/api"
import type { Usage } from "@/lib/types"
import { formatRelative } from "@/lib/utils"
import { useApp } from "@/state/AppState"

/**
 * What the AI tools did with this company's knowledge.
 *
 * The screen exists to answer one question the rest of the product cannot:
 * is anybody actually using this. Everywhere else reports what Knowlith
 * knows; this reports what left it, to whom, and when.
 *
 * Two things here are deliberate.
 *
 * **It never claims to explain an answer.** It says what was read. A model
 * can answer from its own context without calling a tool at all, and
 * "Claude answered this because it read your rule" would be a claim the
 * gateway has no way to check.
 *
 * **What was skipped is shown as prominently as what was read.** That line
 * is the one thing no retrieval system can produce: the approved set is
 * finite, the gateway said out loud what touched the question, and so the
 * things the agent was offered and never opened can be named.
 */
const POLL_MS = 5000

export function Activity() {
  const { companyName } = useApp()
  const [rows, setRows] = useState<Usage[] | null>(null)

  useEffect(() => {
    let cancelled = false
    const poll = async () => {
      const next = await toolsApi.usage()
      if (!cancelled) setRows(next)
    }
    void poll()
    const timer = window.setInterval(poll, POLL_MS)
    return () => {
      cancelled = true
      window.clearInterval(timer)
    }
  }, [])

  return (
    <div className="mx-auto w-full max-w-[880px] px-4 py-8">
      <h1 className="text-[20px] font-semibold tracking-[-0.015em] text-ink">Activity</h1>
      <p className="mt-1 max-w-[62ch] text-[13px] text-muted">
        What the AI tools on this computer read from {companyName}. Recorded by the gateway as it
        served them, so this is what left Knowlith — not what any tool did with it afterwards.
      </p>

      {rows === null ? (
        <p className="mt-8 text-[13px] text-muted">Reading…</p>
      ) : rows.length === 0 ? (
        <Panel className="mt-7">
          <PanelHeader title="Nothing has been read yet" />
          <p className="px-4 pb-4 text-[13px] text-muted">
            Connect a tool under AI tools and ask it one question. When it reads something from
            {" "}
            {companyName}, it appears here — with the thing it read, and the things it was offered
            and skipped.
          </p>
        </Panel>
      ) : (
        <ul className="mt-7 space-y-2.5">
          {rows.map((row) => (
            <li key={row.id} className="rounded-xl border border-line bg-surface p-4">
              <div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1">
                <span className="text-[13.5px] font-medium text-ink">{row.appLabel}</span>
                <span className="text-[12px] text-faint">{formatRelative(row.at)}</span>
              </div>

              {row.question ? (
                <p className="mt-1 text-[13px] text-muted">
                  Working on: <span className="text-ink">{row.question}</span>
                </p>
              ) : null}

              <Named
                Icon={Eye}
                tone="text-confirmed"
                label="Read"
                objects={row.read}
              />

              {row.skipped.length > 0 ? (
                <Named
                  Icon={EyeOff}
                  tone="text-pending"
                  label="Offered and not opened"
                  objects={row.skipped}
                />
              ) : null}

              {/* Only meaningful for a declared case: an agent that never
                  opened one was never offered a coverage check, so saying
                  it failed to run one would be blaming it for our design. */}
              {row.question ? (
                <p className="mt-2.5 flex items-center gap-1.5 text-[12px]">
                  {row.closed ? (
                    <>
                      <Check className="size-3.5 shrink-0 text-confirmed" />
                      <span className="text-muted">Checked what it had missed before answering.</span>
                    </>
                  ) : (
                    <>
                      <AlertTriangle className="size-3.5 shrink-0 text-pending" />
                      <span className="text-muted">
                        Answered without checking what it had missed.
                      </span>
                    </>
                  )}
                </p>
              ) : null}
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}

/** How many of a list are shown before it is folded away. */
const SHOWN = 5

function Named({
  Icon,
  tone,
  label,
  objects,
}: {
  Icon: typeof Eye
  tone: string
  label: string
  objects: Usage["read"]
}) {
  const [all, setAll] = useState(false)
  if (objects.length === 0) return null

  // A company with fifty approved rules produces a skipped list that buries
  // the one line above it — which is what the owner came to read. The count
  // is the information; the names are there for whoever wants them.
  const shown = all ? objects : objects.slice(0, SHOWN)
  const rest = objects.length - shown.length

  return (
    <div className="mt-2.5 flex gap-2.5">
      <Icon className={`mt-0.5 size-3.5 shrink-0 ${tone}`} />
      <div className="min-w-0">
        <span className="text-[12px] text-faint">
          {label} · {objects.length}
        </span>
        <ul className="mt-0.5 space-y-0.5">
          {shown.map((object) => (
            <li key={object.id} className="text-[13px] text-ink">
              {object.title}
              <span className="ml-1.5 text-[12px] text-faint">{object.kind}</span>
            </li>
          ))}
        </ul>
        {rest > 0 ? (
          <button
            type="button"
            onClick={() => setAll(true)}
            className="mt-1 text-[12px] text-muted underline-offset-4 hover:text-ink hover:underline"
          >
            and {rest} more
          </button>
        ) : null}
      </div>
    </div>
  )
}
