import { Suspense, lazy, useEffect, useMemo, useState } from "react"
import { AlertTriangle, ArrowRight, Check, FileText, Loader2, PauseCircle, RefreshCw, Search, ShieldCheck } from "lucide-react"
import { background, brain, failed } from "@/lib/api"
import type { BrainNode, BuildBrain, Processor, Work } from "@/lib/types"
import { buildState } from "@/lib/buildState"
import { BRAIN_KIND_COLOR } from "@/lib/brainGraph"
import { cn, formatCount } from "@/lib/utils"
import { useApp } from "@/state/AppState"
import { Button } from "@/components/ui/button"

const BrainGraph3D = lazy(() => import("@/components/BrainGraph3D").then((m) => ({ default: m.BrainGraph3D })))
const ENGINE = { codex: "Codex", "claude-code": "Claude Code", "cursor-agent": "Cursor Agent", managed: "Your reader" }

export function StepBuilding({ company, processor, onDone }: {
  company: string; processor: Processor; onDone: () => void
}) {
  const [graph, setGraph] = useState<BuildBrain | null>(null)
  const [work, setWork] = useState<Work | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [actionError, setActionError] = useState<string | null>(null)
  const [selected, setSelected] = useState<BrainNode | null>(null)
  const [query, setQuery] = useState("")
  const [releasing, setReleasing] = useState(false)
  const [reset, setReset] = useState(0)
  const { refresh } = useApp()

  useEffect(() => {
    let cancelled = false
    let timer = 0
    let request: AbortController | null = null
    const poll = async () => {
      request = new AbortController()
      const timeout = window.setTimeout(() => request?.abort(), 10_000)
      try {
        const next = await brain.build(request.signal)
        if (cancelled) return
        setError(null)
        setWork(next.work)
        // Polling the queue should not rebuild an unchanged GPU scene.
        setGraph((previous) => JSON.stringify(previous) === JSON.stringify(next.graph) ? previous : next.graph)
      } catch {
        if (!cancelled) setError("Connection interrupted. Your saved discoveries are safe; reconnecting…")
      } finally {
        window.clearTimeout(timeout)
        if (!cancelled) timer = window.setTimeout(() => void poll(), document.hidden ? 10_000 : 1500)
      }
    }
    void poll()
    return () => { cancelled = true; window.clearTimeout(timer); request?.abort() }
  }, [])

  const counts = graph?.counts ?? { documents: 0, discoveries: 0, approved: 0 }
  const findings = counts.discoveries + counts.approved
  const state = buildState(work, findings)
  const working = state === "working" && !error
  const ready = state === "ready" && !error
  const nodes = useMemo(() => (graph?.nodes ?? []).filter((n) => n.title.toLowerCase().includes(query.toLowerCase())), [graph, query])
  const current = selected ? graph?.nodes.find((n) => n.id === selected.id) ?? null : null
  const connections = current ? graph?.edges.filter((e) => e.from === current.id || e.to === current.id) ?? [] : []
  const title = error ? "Reconnecting to your build" : state === "ready" ? "Ready for your review" : state === "paused" ? "Your build is paused" : state === "attention" ? "Some files need attention" : state === "empty" ? "Waiting for discoveries" : "Your company brain is taking shape"

  const release = async () => {
    setReleasing(true)
    setActionError(null)
    try {
      const currentPolicy = await background.policy()
      if (!currentPolicy) throw new Error("Could not read your processing settings.")
      const saved = await background.setPolicy({ ...currentPolicy.policy, processing: "automatic", pauseOnBattery: false })
      if (!saved) throw new Error("Could not save your processing settings.")
      const result = await background.release()
      if (failed(result)) throw new Error(result.error)
    } catch (e) { setActionError(e instanceof Error ? e.message : "Could not resume the build.") }
    finally { setReleasing(false) }
  }

  const review = async () => {
    setReleasing(true)
    try { await refresh(); onDone() }
    catch { setActionError("Could not open review. Please try again.") }
    finally { setReleasing(false) }
  }

  return (
    <div>
      <div className="mb-7 flex flex-wrap items-start justify-between gap-4">
        <div>
          <p className="mb-2 text-[11px] font-medium uppercase tracking-[0.16em] text-accent">{company} / Knowledge build</p>
          <h1 className="max-w-[24ch] text-[30px] font-semibold leading-[1.15] tracking-[-0.03em] text-ink">{title}</h1>
          <p className="mt-3 max-w-[56ch] text-[14px] leading-relaxed text-muted">Watch your documents become connected knowledge. You decide what becomes part of the company brain.</p>
        </div>
        <span className="inline-flex items-center gap-2 rounded-full border border-line px-3 py-1.5 text-xs text-muted"><span className={cn("size-1.5 rounded-full", working ? "bg-accent" : "bg-faint")} />{ENGINE[processor]}</span>
      </div>

      <div className="overflow-hidden rounded-2xl border border-line bg-surface shadow-k">
        <div className="grid grid-cols-3 border-b border-line bg-surface-2/50">
          {[[counts.documents, "Documents read"], [counts.discoveries, "Awaiting review"], [counts.approved, "Approved knowledge"]].map(([value, label]) => (
            <div key={label} className="px-4 py-4 sm:px-6"><div className="tabular text-[26px] font-semibold tracking-tight text-ink">{formatCount(Number(value))}</div><div className="mt-1 text-[11px] text-muted sm:text-xs">{label}</div></div>
          ))}
        </div>
        <div className="relative h-[330px] sm:h-[450px]">
          <Suspense fallback={<div className="grid h-full place-items-center text-muted">Preparing the 3D view…</div>}>
            <BrainGraph3D nodes={graph?.nodes ?? []} edges={graph?.edges ?? []} selectedId={current?.id ?? null} kindFilter="all" resetSignal={reset} growing onSelect={setSelected} onOpen={setSelected} />
          </Suspense>
          <div className="pointer-events-none absolute inset-x-0 top-0 flex items-start justify-between p-4">
            <span className="rounded-full border border-line bg-surface/90 px-2.5 py-1 text-[11px] text-muted">{working ? "Live discoveries" : "Saved discoveries"}</span>
            <Button className="pointer-events-auto" size="sm" variant="subtle" onClick={() => setReset((v) => v + 1)}><RefreshCw className="size-3.5" />Fit view</Button>
          </div>
          {counts.documents === 0 ? <div className="pointer-events-none absolute inset-x-4 bottom-12 text-center text-[13px] text-muted">Your first document will appear here as it is read.</div> : null}
          <div className="pointer-events-none absolute inset-x-0 bottom-0 flex flex-wrap justify-center gap-x-5 gap-y-1 bg-surface/85 px-4 py-3 text-[11px] text-muted">
            <span>◇ Awaiting review</span><span>● Approved or source</span><span>Drag to rotate · scroll to zoom</span>
          </div>
        </div>
        <div className="flex items-start gap-3 border-t border-line bg-surface-2/50 px-4 py-4 sm:px-6" role="status" aria-live="polite">
          {error || state === "attention" ? <AlertTriangle className="mt-0.5 size-4 shrink-0 text-pending" /> : state === "paused" ? <PauseCircle className="mt-0.5 size-4 shrink-0 text-pending" /> : ready ? <Check className="mt-0.5 size-4 shrink-0 text-confirmed" /> : <Loader2 className={cn("mt-0.5 size-4 shrink-0 text-accent", working && "animate-spin motion-reduce:animate-none")} />}
          <div className="min-w-0 flex-1"><p className="text-[13px] font-medium text-ink">{error ?? (ready ? "The current work is complete. Review what was found." : state === "paused" ? work?.held?.reason : work?.doing ?? "Connecting to your build…")}</p>
            <p className="mt-1 text-xs text-muted">{work && work.total > 0 ? `${work.done} / ${work.total} jobs finished in the current stage. ` : ""}You can close this tab. Your build continues on this computer.</p>
            {state === "paused" ? <Button className="mt-3" size="sm" disabled={releasing} onClick={() => void release()}>{releasing ? "Resuming…" : "Resume reading"}</Button> : null}
          </div>
        </div>
      </div>

      <div className="mt-5 grid gap-5 sm:grid-cols-2">
        <section className="min-w-0 rounded-xl border border-line p-4">
          <h2 className="text-[13px] font-semibold text-ink">{current ? current.title : "Explore what is forming"}</h2>
          {current ? <div className="mt-2 text-xs text-muted"><p>{current.kind === "document" ? "Extracted source document" : current.status === "approved" ? "Approved knowledge" : "Discovery · awaiting your review"}</p><p className="mt-1">Connections: {connections.length}</p><button className="mt-2 text-accent underline" onClick={() => setSelected(null)}>Show all discoveries</button></div> : null}
          <label className="mt-3 flex items-center gap-2 rounded-lg border border-line px-2.5 py-2"><Search className="size-3.5 text-faint" /><input aria-label="Find a discovery" value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Find a document or discovery" className="w-full min-w-0 bg-transparent text-xs text-ink outline-none" /></label>
          <ul className="mt-2 max-h-[190px] overflow-y-auto">
            {nodes.slice(0, 100).map((node) => <li key={node.id}><button onClick={() => setSelected(node)} className={cn("flex w-full items-center gap-2 rounded-md px-2 py-2 text-left text-xs hover:bg-surface-2", current?.id === node.id ? "bg-accent-soft text-ink" : "text-muted")}><span className="size-1.5 shrink-0 rounded-full" style={{ background: BRAIN_KIND_COLOR[node.kind] }} /><span className="truncate">{node.title}</span></button></li>)}
            {nodes.length === 0 ? <li className="py-4 text-xs text-faint">{query ? "No matching discoveries." : "Real documents and findings will appear here."}</li> : null}
          </ul>
          {nodes.length > 100 ? <p className="mt-2 text-xs text-faint">Showing the first 100 matches. Search to narrow the list.</p> : null}
        </section>
        <section className="min-w-0 rounded-xl border border-line p-4"><h2 className="text-[13px] font-semibold text-ink">Build activity</h2><ol className="mt-3 max-h-[235px] space-y-3 overflow-y-auto">
          {(work?.lines ?? []).slice(0, 10).map((line, i) => <li key={`${line.at}-${line.subject}-${i}`} className="flex items-start gap-2.5">{line.state === "failed" ? <AlertTriangle className="mt-0.5 size-3.5 shrink-0 text-pending" /> : line.state === "working" ? <Loader2 className="mt-0.5 size-3.5 shrink-0 animate-spin text-accent motion-reduce:animate-none" /> : <FileText className="mt-0.5 size-3.5 shrink-0 text-faint" />}<div className="min-w-0"><p className="truncate text-xs font-medium text-ink">{line.subject}</p><p className="mt-0.5 break-words text-xs leading-relaxed text-muted">{line.note}</p></div></li>)}
          {!work?.lines.length ? <li className="text-xs text-faint">Updates appear as the worker reads your files.</li> : null}
        </ol></section>
      </div>
      {actionError ? <p role="alert" className="mt-4 text-sm text-conflict">{actionError}</p> : null}
      <div className="mt-6 flex flex-wrap items-center justify-between gap-4 rounded-xl bg-accent-soft px-4 py-4">
        <p className="flex max-w-[48ch] items-start gap-2 text-xs leading-relaxed text-muted"><ShieldCheck className="mt-0.5 size-4 shrink-0 text-accent" />Discoveries stay private to this review. Your AI tools receive knowledge only after you approve it.</p>
        {findings > 0 && (ready || state === "attention") && !error ? <Button variant="primary" disabled={releasing} onClick={() => void review()}>{state === "attention" ? "Review available findings" : "Review discoveries"}<ArrowRight /></Button> : null}
      </div>
    </div>
  )
}
