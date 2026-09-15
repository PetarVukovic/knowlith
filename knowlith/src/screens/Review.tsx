import { useState } from "react"
import { useNavigate } from "react-router-dom"
import { ArrowRight, Check, Combine, GitMerge, Layers, Lightbulb, Pencil, PartyPopper, X } from "lucide-react"
import { Confidence, ImpactStrip, KindIcon, RelationList, kindMeta } from "@/components/Domain"
import { DiffView } from "@/components/DiffView"
import { EvidenceCard, EvidenceList } from "@/components/Evidence"
import { ResizeHandle, usePanelSize } from "@/components/Resizable"
import { Button } from "@/components/ui/button"
import { Textarea } from "@/components/ui/input"
import { Panel } from "@/components/ui/surface"
import { cn, formatRelative } from "@/lib/utils"
import { useApp } from "@/state/AppState"

/**
 * A hint shown once, during the first review.
 *
 * Three sentences is the whole tutorial: what this is, where it came from,
 * what approving does. They sit inline above the thing they describe rather
 * than floating over it, so nothing is covered and nothing has to be dismissed
 * before the owner can read the change they are deciding on.
 */
function Coachmark({ children }: { children: React.ReactNode }) {
  return (
    <p className="mb-2 flex items-start gap-2 rounded-lg border border-accent/25 bg-accent-soft px-3 py-2 text-[12.5px] leading-relaxed text-ink">
      <Lightbulb className="mt-0.5 size-3.5 shrink-0 text-accent" />
      {children}
    </p>
  )
}

/**
 * Pairs the daemon could not decide about.
 *
 * These sit above the queue rather than inside it, because they are a
 * different question: the queue asks "is this true", this asks "are these
 * two the same thing". Mixing them would make one Approve button mean two
 * things.
 */
function MergeHints() {
  const { mergeHints, mergeObjects, keepBoth, mode } = useApp()
  if (mergeHints.length === 0) return null

  return (
    <div className="mb-5 grid gap-2.5">
      {mergeHints.map((hint) => {
        // Two different questions, so two different tones. One asks whether
        // to tidy something up; the other says two answers to one question
        // are live right now, and until it is settled an AI tool asked today
        // will give whichever it happened to read first.
        const disagreement = hint.kind === "disagreement"
        return (
        <Panel
          key={`${hint.keepId}-${hint.dropId}`}
          className={disagreement ? "overflow-hidden border-conflict/30" : "overflow-hidden border-pending/30"}
        >
          <div
            className={
              disagreement
                ? "flex items-center gap-2 border-b border-conflict/20 bg-conflict-soft px-3.5 py-2"
                : "flex items-center gap-2 border-b border-pending/20 bg-pending-soft px-3.5 py-2"
            }
          >
            {disagreement ? (
              <GitMerge className="size-3.5 text-conflict" />
            ) : (
              <Combine className="size-3.5 text-pending" />
            )}
            <span
              className={
                disagreement
                  ? "text-[12.5px] font-medium text-conflict"
                  : "text-[12.5px] font-medium text-pending"
              }
            >
              {disagreement
                ? "Two answers to the same question are in use"
                : "These may be the same thing, written twice"}
            </span>
            {mode === "engineer" ? (
              <span className="tabular ml-auto font-mono text-[11px] text-faint">
                {Math.round(hint.score * 100)}% overlap
              </span>
            ) : null}
          </div>
          <div className="grid gap-2 p-3.5 sm:grid-cols-2">
            {[
              { id: hint.keepId, title: hint.keepTitle, body: hint.keepBody, kept: true },
              { id: hint.dropId, title: hint.dropTitle, body: hint.dropBody, kept: false },
            ].map((side) => (
              <div key={side.id} className="rounded-md border border-line bg-surface-2 p-2.5">
                <div className="flex items-baseline gap-2">
                  <span className="min-w-0 flex-1 truncate text-[12.5px] font-medium text-ink">{side.title}</span>
                  {side.kept ? <span className="shrink-0 text-[11px] text-confirmed">kept</span> : null}
                </div>
                <p className="mt-1.5 text-[12px] leading-relaxed text-muted">{side.body}</p>
              </div>
            ))}
          </div>
          <div className="flex flex-wrap items-center gap-2 border-t border-line px-3.5 py-2.5">
            <Button variant="primary" size="sm" onClick={() => mergeObjects(hint.keepId, hint.dropId)}>
              <Combine />
              {disagreement ? "Keep the first one" : "Merge into one"}
            </Button>
            <Button variant="ghost" size="sm" onClick={() => keepBoth(hint.keepId, hint.dropId)}>
              They are different
            </Button>
            <span className="text-[12px] text-faint">
              {disagreement
                ? "The other stays readable as history. Neither file is changed."
                : "Merging keeps both sources. Nothing is deleted."}
            </span>
          </div>
        </Panel>
        )
      })}
    </div>
  )
}

export function Review() {
  const { review, approve, reject, mode, firstRun, setFirstRun } = useApp()
  const navigate = useNavigate()
  const guided = firstRun === "review"
  const [justApproved, setJustApproved] = useState(false)
  const queue = usePanelSize("review-queue", 280, 200, 460)
  const [pickedId, setPickedId] = useState<string | null>(null)
  /** Keyed by item id so switching items drops the edit without an effect. */
  const [draftFor, setDraftFor] = useState<{ id: string; text: string } | null>(null)

  // Derived, not stored: the queue shrinks as items are decided, and the
  // selection has to follow it without a render-then-correct round trip.
  const item = review.find((r) => r.id === pickedId) ?? review[0] ?? null
  const selectedId = item?.id ?? null
  const setSelectedId = setPickedId
  const draft = draftFor && draftFor.id === selectedId ? draftFor.text : null
  const setDraft = (text: string | null) =>
    setDraftFor(text === null || selectedId === null ? null : { id: selectedId, text })

  if (guided && justApproved) {
    return (
      <div className="mx-auto w-full max-w-[560px] px-5 py-16 text-center">
        <span className="mx-auto grid size-12 place-items-center rounded-xl bg-confirmed-soft text-confirmed">
          <PartyPopper className="size-5" />
        </span>
        <h1 className="mt-5 text-[24px] font-semibold leading-tight tracking-[-0.022em] text-ink">
          Your first company rule is now live.
        </h1>
        <p className="mx-auto mt-3 max-w-[42ch] text-[14px] leading-relaxed text-muted">
          Every AI tool you connect will answer from it, and will say which document it came from. The source
          file was not touched.
        </p>
        <div className="mt-8 flex flex-wrap items-center justify-center gap-3">
          <Button
            size="lg"
            variant="primary"
            onClick={() => {
              setFirstRun("connect")
              navigate("/connect")
            }}
          >
            Connect an AI tool
            <ArrowRight />
          </Button>
          <Button
            size="lg"
            variant="ghost"
            onClick={() => setJustApproved(false)}
          >
            Keep reviewing
          </Button>
        </div>
      </div>
    )
  }

  if (review.length === 0) {
    return (
      <div className="grid min-h-full place-items-center px-4 py-16">
        <div className="max-w-[42ch] text-center">
          <PartyPopper className="mx-auto size-6 text-confirmed" />
          <h1 className="mt-3 text-[17px] font-semibold text-ink">Nothing is waiting</h1>
          <p className="mt-1.5 text-[13px] text-muted">
            Every change has been decided. New ones appear here the next time Knowlith reads your folders.
          </p>
        </div>
      </div>
    )
  }

  return (
    <div className="flex min-h-full">
      <div className="hidden shrink-0 bg-bg md:block" style={{ width: queue.width }}>
        <div className="px-3 py-3 label-xs">Waiting · {review.length}</div>
        <ul className="grid gap-px px-2 pb-3">
          {review.map((r) => (
            <li key={r.id}>
              <button
                type="button"
                onClick={() => setSelectedId(r.id)}
                className={cn(
                  "w-full rounded-md px-2 py-2 text-left",
                  r.id === selectedId ? "bg-surface shadow-k ring-1 ring-line" : "hover:bg-surface-3",
                )}
              >
                <span className="flex items-center gap-1.5">
                  <KindIcon kind={r.kind} />
                  <span className="min-w-0 flex-1 truncate text-[12.5px] font-medium text-ink">{r.title}</span>
                  {r.conflict ? <GitMerge className="size-3 shrink-0 text-conflict" /> : null}
                </span>
                <span className="mt-1 flex items-center gap-2 pl-5">
                  <span className="text-[11.5px] text-faint">{r.before ? "changed" : "new"}</span>
                  <Confidence value={r.confidence} />
                </span>
              </button>
            </li>
          ))}
        </ul>
      </div>
      <ResizeHandle panel={queue} edge="start" label="Resize the waiting list" className="hidden md:block" />

      {item ? (
        <div className="min-w-0 flex-1 px-4 py-6 sm:px-7">
          <div className="mx-auto max-w-[840px]">
            <div className="scroll-thin -mx-4 mb-4 flex gap-1.5 overflow-x-auto px-4 pb-1 md:hidden">
              {review.map((r) => (
                <button
                  key={r.id}
                  type="button"
                  onClick={() => setSelectedId(r.id)}
                  className={cn(
                    "shrink-0 rounded-md border px-2.5 py-1 text-[12px]",
                    r.id === selectedId
                      ? "border-accent bg-accent-soft text-accent"
                      : "border-line bg-surface text-muted",
                  )}
                >
                  {r.title}
                </button>
              ))}
            </div>

            <MergeHints />

            <div className="flex flex-wrap items-center gap-2">
              <KindIcon kind={item.kind} />
              <h1 className="text-[18px] font-semibold tracking-[-0.01em] text-ink">{item.title}</h1>
              <span className="text-[12px] text-faint">{kindMeta(item.kind).label}</span>
              <span className="ml-auto text-[12px] text-faint">
                {mode === "engineer" ? "compiled" : "found"} {formatRelative(item.compiledAt)}
              </span>
            </div>

            <p className="mt-1.5 text-[13px] text-muted">
              {item.before
                ? "Knowlith found that what it had on file no longer matches your documents."
                : "Knowlith found something it did not know before."}{" "}
              <Confidence value={item.confidence} />
            </p>

            {item.conflict ? (
              <Panel className="mt-5 overflow-hidden border-conflict/30">
                <div className="flex items-center gap-2 border-b border-conflict/20 bg-conflict-soft px-3.5 py-2">
                  <GitMerge className="size-3.5 text-conflict" />
                  <span className="text-[12.5px] font-medium text-conflict">Two documents disagree</span>
                </div>
                <div className="p-3.5">
                  <p className="text-[12.5px] text-muted">{item.conflict.summary}</p>
                  <div className="mt-3 grid gap-2 sm:grid-cols-2">
                    {item.conflict.sides.map((side) => (
                      <div key={side.label} className="rounded-md border border-line bg-surface-2 p-2.5">
                        <div className="flex items-baseline justify-between gap-2">
                          <span className="truncate text-[12px] text-muted">{side.label}</span>
                          <span className="tabular shrink-0 text-[15px] font-semibold text-ink">{side.value}</span>
                        </div>
                        <p className="mt-1.5 border-l-2 border-line pl-2 text-[12px] leading-relaxed text-muted">
                          {side.evidence.quote}
                        </p>
                      </div>
                    ))}
                  </div>
                  <p className="mt-3 text-[12px] text-muted">
                    Knowlith keeps the newer document and marks the older one as out of circulation. Approving does not
                    change either file.
                  </p>
                </div>
              </Panel>
            ) : null}

            {item.coverage ? (
              <Panel className="mt-5 overflow-hidden">
                <div className="flex items-center gap-2 border-b border-line bg-surface-2 px-3.5 py-2">
                  <Layers className="size-3.5 text-faint" />
                  <span className="text-[12.5px] font-medium text-ink">
                    More than one document covers this
                  </span>
                </div>
                <div className="p-3.5">
                  {/* Deliberately not called a disagreement. Neither document
                      states a figure, so nothing here can establish that they
                      contradict each other — only that both speak to it. */}
                  <p className="text-[12.5px] text-muted">{item.coverage.summary}</p>
                  <ul className="mt-3 grid gap-2">
                    {item.coverage.sources.map((source) => (
                      <li key={source.evidence.id} className="rounded-md border border-line bg-surface-2 p-2.5">
                        <div className="flex flex-wrap items-baseline gap-x-2 gap-y-1">
                          <span className="min-w-0 flex-1 truncate text-[12px] text-muted">{source.label}</span>
                          {source.current ? (
                            <span className="shrink-0 text-[11px] text-confirmed">wording in use</span>
                          ) : (
                            <span className="shrink-0 text-[11px] text-faint">also says</span>
                          )}
                        </div>
                        <p className="mt-1.5 border-l-2 border-line pl-2 text-[12px] leading-relaxed text-muted">
                          {source.evidence.quote}
                        </p>
                      </li>
                    ))}
                  </ul>
                </div>
              </Panel>
            ) : null}

            <div className="mt-6 grid gap-5 lg:grid-cols-[1fr_320px]">
              <section className="min-w-0">
                {guided ? <Coachmark>This is what Knowlith believes is current.</Coachmark> : null}
                <div className="mb-2 flex items-center justify-between gap-2">
                  <h2 className="text-[13px] font-semibold text-ink">
                    {item.before ? "What changes" : "What gets added"}
                  </h2>
                  {draft === null ? (
                    <Button variant="ghost" size="sm" onClick={() => setDraft(item.after)}>
                      <Pencil />
                      Edit before approving
                    </Button>
                  ) : (
                    <Button variant="ghost" size="sm" onClick={() => setDraft(null)}>
                      <X />
                      Discard my edit
                    </Button>
                  )}
                </div>

                {draft === null ? (
                  <DiffView before={item.before} after={item.after} />
                ) : (
                  <Textarea rows={14} value={draft} onChange={(e) => setDraft(e.target.value)} />
                )}

                <h2 className="mb-2 mt-6 text-[13px] font-semibold text-ink">What this affects</h2>
                <ImpactStrip relations={item.affects} objectId={item.objectId} className="mb-2" />
                <Panel className="p-2">
                  <RelationList relations={item.affects} emptyLabel="Nothing else uses this yet." />
                </Panel>
                {item.affects.length > 0 ? (
                  <p className="mt-2 text-[12px] text-muted">
                    Approving updates these immediately. Anything already sent out stays as it was.
                  </p>
                ) : null}
              </section>

              <aside className="min-w-0">
                {guided ? <Coachmark>These are the exact sources behind it.</Coachmark> : null}
                <h2 className="mb-2 text-[13px] font-semibold text-ink">Where it comes from</h2>
                <EvidenceList items={item.evidence} />
                {item.conflict ? (
                  <div className="mt-3">
                    <div className="mb-1.5 label-xs">Out of circulation</div>
                    <EvidenceCard evidence={item.conflict.sides[0].evidence} />
                  </div>
                ) : null}
                {mode === "engineer" ? (
                  <p className="mt-3 font-mono text-[11px] text-faint">
                    {item.objectId} · conf {item.confidence.toFixed(2)} · {item.evidence.length} spans
                  </p>
                ) : null}
              </aside>
            </div>

            {guided ? (
              <div className="mt-8">
                <Coachmark>
                  Approve makes this available to your AI tools. It does not modify the source files.
                </Coachmark>
              </div>
            ) : null}

            <div
              className={cn(
                "sticky bottom-0 flex flex-wrap items-center gap-2 border-t border-line bg-bg py-3",
                guided ? "mt-0" : "mt-8",
              )}
            >
              <Button
                variant="primary"
                onClick={() => {
                  approve(item.id, draft !== null)
                  if (guided) setJustApproved(true)
                }}
              >
                <Check />
                {draft !== null ? "Approve my version" : "Approve"}
              </Button>
              <Button variant="danger" onClick={() => reject(item.id)}>
                <X />
                Reject
              </Button>
              <span className="text-[12px] text-faint">
                Approving puts this in front of every AI tool you have connected.
              </span>
            </div>
          </div>
        </div>
      ) : (
        <div className="min-w-0 flex-1 px-4 py-6 sm:px-7">
          <div className="mx-auto max-w-[840px]">
            {/* A merge question outlives the queue: the pair can still be
                waiting after the last item has been decided, and before this
                branch existed it simply became unreachable. */}
            <MergeHints />
            <div className="rounded-xl border border-line bg-surface px-5 py-8 text-center">
              <span className="mx-auto grid size-10 place-items-center rounded-lg bg-confirmed-soft text-confirmed">
                <Check className="size-4" />
              </span>
              <h1 className="mt-4 text-[16px] font-semibold text-ink">Nothing is waiting for you.</h1>
              <p className="mx-auto mt-1.5 max-w-[46ch] text-[13px] leading-relaxed text-muted">
                Knowlith keeps reading in the background. Anything new from your documents will show up here.
              </p>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
