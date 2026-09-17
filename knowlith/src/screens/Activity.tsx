import { useEffect, useMemo, useState } from "react"
import { useNavigate } from "react-router-dom"
import { Check, ChevronRight } from "lucide-react"
import { Panel, PanelHeader } from "@/components/ui/surface"
import { api, tools as toolsApi } from "@/lib/api"
import type { EngineRun, Usage } from "@/lib/types"
import { cn, formatRelative } from "@/lib/utils"
import { useApp } from "@/state/AppState"

const POLL_MS = 5000

type FoundRow = {
  id: string
  title: string
  detail: string
  at: string
  tone: string
}

type FeedItem =
  | { kind: "used"; at: string; row: Usage }
  | { kind: "found"; at: string; row: FoundRow }
  | { kind: "spend"; at: string; row: EngineRun }

/**
 * What left Knowlith — in owner language.
 *
 * Never claims why a model answered the way it did. Only what was found,
 * what the team confirmed, what assistants actually read, and what the
 * engine printed about tokens and price.
 */
export function Activity() {
  const { companyName } = useApp()
  const navigate = useNavigate()
  const [usage, setUsage] = useState<Usage[] | null>(null)
  const [found, setFound] = useState<FoundRow[]>([])
  const [spend, setSpend] = useState<EngineRun[]>([])
  const [openId, setOpenId] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    const poll = async () => {
      const [nextUsage, nextFound, nextSpend] = await Promise.all([
        toolsApi.usage(),
        api.getRecentActivity(),
        api.getEngineRuns(),
      ])
      if (cancelled) return
      setUsage(nextUsage)
      setSpend(nextSpend ?? [])
      setFound(
        (nextFound ?? []).map((row) => ({
          id: row.id,
          title: plainFoundTitle(row.title),
          detail: plainFoundDetail(row.detail),
          at: row.at,
          tone: row.tone,
        })),
      )
    }
    void poll()
    const timer = window.setInterval(poll, POLL_MS)
    return () => {
      cancelled = true
      window.clearInterval(timer)
    }
  }, [])

  const feed = useMemo(() => {
    const items: FeedItem[] = []
    for (const row of usage ?? []) {
      items.push({ kind: "used", at: row.at, row })
    }
    for (const row of found) {
      items.push({ kind: "found", at: row.at, row })
    }
    for (const row of spend) {
      items.push({ kind: "spend", at: row.at, row })
    }
    items.sort((a, b) => b.at.localeCompare(a.at))
    return items
  }, [usage, found, spend])

  return (
    <div className="mx-auto w-full max-w-[720px] px-4 py-8">
      <h1 className="text-[20px] font-semibold tracking-[-0.015em] text-ink">History</h1>
      <p className="mt-1 max-w-[54ch] text-[13px] text-muted">
        Four kinds of moment, in plain words: something new was found in your folders, your team
        confirmed it, an AI assistant read it while answering someone, or Knowlith's engine printed
        what that read cost.
      </p>

      {usage === null ? (
        <p className="mt-8 text-[13px] text-muted">Reading…</p>
      ) : feed.length === 0 ? (
        <Panel className="mt-7">
          <PanelHeader title="Nothing has happened yet" />
          <p className="px-4 pb-4 text-[13px] text-muted">
            Add a folder, confirm a few findings, then ask an assistant about {companyName}. Each of
            those steps shows up here.
          </p>
        </Panel>
      ) : (
        <ul className="mt-7 space-y-2">
          {feed.map((item) => {
            if (item.kind === "spend") {
              const row = item.row
              const detail = spendDetail(row)
              return (
                <li key={row.id} className="rounded-xl border border-line bg-surface px-4 py-3.5">
                  <div className="flex items-start gap-3">
                    <span className="min-w-0 flex-1">
                      <span className="block text-[12px] font-medium uppercase tracking-wide text-faint">
                        Reading your files
                      </span>
                      <span className="mt-0.5 block text-[13.5px] font-medium text-ink">
                        {row.title}
                      </span>
                      {detail ? (
                        <span className="mt-0.5 block font-mono text-[12.5px] tabular-nums text-muted">
                          {detail}
                        </span>
                      ) : null}
                      <span className="mt-1 block text-[12px] text-faint">
                        {formatRelative(row.at)}
                      </span>
                    </span>
                  </div>
                </li>
              )
            }

            if (item.kind === "found") {
              const row = item.row
              return (
                <li key={row.id} className="rounded-xl border border-line bg-surface px-4 py-3.5">
                  <div className="flex items-start gap-3">
                    <span className="min-w-0 flex-1">
                      <span className="block text-[12px] font-medium uppercase tracking-wide text-faint">
                        {row.tone === "conflict"
                          ? "Needs a decision"
                          : row.title.includes("confirmed") || row.title.includes("approved")
                            ? "Your team confirmed"
                            : "Found in your folders"}
                      </span>
                      <span className="mt-0.5 block text-[13.5px] font-medium text-ink">
                        {row.title}
                      </span>
                      {row.detail ? (
                        <span className="mt-0.5 block text-[12.5px] text-muted">{row.detail}</span>
                      ) : null}
                      <span className="mt-1 block text-[12px] text-faint">
                        {formatRelative(row.at)}
                      </span>
                    </span>
                  </div>
                </li>
              )
            }

            const row = item.row
            const open = openId === row.id
            const used = row.read
            const headline =
              used.length === 0
                ? row.question
                  ? `${row.appLabel} asked about “${row.question}”`
                  : `${row.appLabel} opened your company knowledge`
                : used.length === 1
                  ? `${row.appLabel} read “${used[0].title}”`
                  : `${row.appLabel} read ${used.length} pieces of what ${companyName} confirmed`

            return (
              <li key={row.id} className="rounded-xl border border-line bg-surface">
                <button
                  type="button"
                  onClick={() => setOpenId(open ? null : row.id)}
                  className="flex w-full items-start gap-3 px-4 py-3.5 text-left"
                >
                  <span className="min-w-0 flex-1">
                    <span className="block text-[12px] font-medium uppercase tracking-wide text-faint">
                      An AI assistant used it
                    </span>
                    <span className="mt-0.5 block text-[13.5px] font-medium text-ink">
                      {headline}
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
                      What {row.appLabel} actually opened
                    </p>
                    {used.length === 0 ? (
                      <p className="mt-1.5 text-[13px] text-muted">
                        Nothing from the approved list was opened this time
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
                        {row.appLabel} could also have opened {row.skipped.length} other{" "}
                        {row.skipped.length === 1 ? "item" : "items"} and did not.
                      </p>
                    ) : null}

                    {row.question ? (
                      <p className="mt-2 text-[12px] text-faint">
                        {row.closed
                          ? "It checked whether anything relevant was missed before finishing."
                          : "It finished without checking whether anything relevant was missed."}
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

function spendDetail(row: EngineRun): string {
  if (row.phrase.startsWith(`${row.engine} · `)) {
    return row.phrase.slice(row.engine.length + 3)
  }
  return row.phrase === row.engine ? "" : row.phrase
}

function plainFoundTitle(title: string): string {
  return title
    .replace(/ approved$/i, " — confirmed by your team")
    .replace(/ found$/i, " — found in a document")
    .replace(/ — documents disagree$/i, " — two documents disagree")
}

function plainFoundDetail(detail: string): string {
  return detail.replace(/^From /, "Quote from ").replace(/\.$/, "")
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
