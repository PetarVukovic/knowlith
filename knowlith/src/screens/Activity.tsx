import { useEffect, useState } from "react"
import { useNavigate } from "react-router-dom"
import { Check, ChevronRight } from "lucide-react"
import { Panel, PanelHeader } from "@/components/ui/surface"
import { tools as toolsApi } from "@/lib/api"
import type { Usage } from "@/lib/types"
import { cn, formatRelative } from "@/lib/utils"
import { useApp } from "@/state/AppState"

const POLL_MS = 5000

/**
 * What left Knowlith — in owner language.
 *
 * Never claims why a model answered the way it did. Only what was read,
 * and what was offered but not opened (plainly worded).
 */
export function Activity() {
  const { companyName } = useApp()
  const navigate = useNavigate()
  const [rows, setRows] = useState<Usage[] | null>(null)
  const [openId, setOpenId] = useState<string | null>(null)

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
    <div className="mx-auto w-full max-w-[720px] px-4 py-8">
      <h1 className="text-[20px] font-semibold tracking-[-0.015em] text-ink">History</h1>
      <p className="mt-1 max-w-[54ch] text-[13px] text-muted">
        What Knowlith found, what your team confirmed, and which knowledge AI assistants used.
      </p>

      {rows === null ? (
        <p className="mt-8 text-[13px] text-muted">Reading…</p>
      ) : rows.length === 0 ? (
        <Panel className="mt-7">
          <PanelHeader title="Nothing has been used yet" />
          <p className="px-4 pb-4 text-[13px] text-muted">
            Connect an assistant and ask it one question about {companyName}. When it reads something
            here, it appears on this list.
          </p>
        </Panel>
      ) : (
        <ul className="mt-7 space-y-2">
          {rows.map((row) => {
            const open = openId === row.id
            const used = row.read
            const summary =
              used.length === 0
                ? row.question
                  ? `asked Knowlith about “${row.question}”`
                  : "opened Knowlith"
                : used.length === 1
                  ? `used “${used[0].title}”`
                  : `used ${used.length} pieces of company knowledge`

            return (
              <li key={row.id} className="rounded-xl border border-line bg-surface">
                <button
                  type="button"
                  onClick={() => setOpenId(open ? null : row.id)}
                  className="flex w-full items-start gap-3 px-4 py-3.5 text-left"
                >
                  <span className="min-w-0 flex-1">
                    <span className="block text-[13.5px] font-medium text-ink">
                      {row.appLabel} {summary}
                    </span>
                    <span className="mt-0.5 block text-[12px] text-faint">
                      {formatRelative(row.at)}
                    </span>
                  </span>
                  <ChevronRight
                    className={cn(
                      "mt-0.5 size-4 shrink-0 text-faint transition-transform",
                      open && "rotate-90",
                    )}
                  />
                </button>

                {open ? (
                  <div className="border-t border-line px-4 py-3">
                    <p className="text-[12.5px] font-medium text-ink">
                      What {row.appLabel} got from Knowlith
                    </p>
                    {used.length === 0 ? (
                      <p className="mt-1.5 text-[13px] text-muted">
                        Nothing was read this time
                        {row.question ? ` while working on “${row.question}”` : ""}.
                      </p>
                    ) : (
                      <ul className="mt-2 grid gap-1">
                        {used.map((object) => (
                          <li key={object.id}>
                            <button
                              type="button"
                              onClick={() =>
                                navigate(
                                  object.kind === "skill"
                                    ? `/skills/${encodeURIComponent(object.id)}`
                                    : `/workspace/${encodeURIComponent(object.id)}`,
                                )
                              }
                              className="flex w-full items-baseline gap-2 rounded-sm py-0.5 text-left text-[13px] text-ink hover:underline"
                            >
                              <Check className="mt-0.5 size-3.5 shrink-0 text-confirmed" />
                              <span className="min-w-0 flex-1 truncate">{object.title}</span>
                              <span className="shrink-0 text-[12px] text-faint">
                                {kindLabel(object.kind)}
                              </span>
                            </button>
                          </li>
                        ))}
                      </ul>
                    )}

                    {row.skipped.length > 0 ? (
                      <p className="mt-3 text-[12.5px] text-muted">
                        {row.appLabel} also had access to {row.skipped.length} other{" "}
                        {row.skipped.length === 1 ? "item" : "items"} and did not open{" "}
                        {row.skipped.length === 1 ? "it" : "them"}.
                      </p>
                    ) : null}

                    {row.question ? (
                      <p className="mt-2 text-[12px] text-faint">
                        {row.closed
                          ? "Checked what it had missed before finishing."
                          : "Finished without checking what it had missed."}
                      </p>
                    ) : null}
                  </div>
                ) : null}
              </li>
            )
          })}
        </ul>
      )}
    </div>
  )
}

function kindLabel(kind: string): string {
  switch (kind) {
    case "rule":
      return "Rule"
    case "process":
      return "Process"
    case "skill":
      return "AI skill"
    case "term":
      return "Term"
    default:
      return "Business info"
  }
}
