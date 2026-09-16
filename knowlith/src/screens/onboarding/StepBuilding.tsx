import { useEffect, useMemo, useRef, useState } from "react"
import { AlertTriangle, Check, ChevronDown, Loader2, PauseCircle } from "lucide-react"
import { api, background, getWorkFeed } from "@/lib/api"
import type { Processor, Work, WorkLine } from "@/lib/types"
import { cn, formatCount, formatRelative } from "@/lib/utils"
import { useApp } from "@/state/AppState"
import { Button } from "@/components/ui/button"

/**
 * The first run, described in the owner's language.
 *
 * The company brain is the focus: it grows from the real work feed, node by
 * node. Progress and every sentence come from `/api/work` — never invented.
 */
const PHASES = [
  { key: "read", label: "Reading company files", detail: "Opening each document and keeping its structure." },
  { key: "terms", label: "Finding company terms", detail: "Words your company uses in its own way." },
  { key: "rules", label: "Identifying business rules", detail: "Limits, deadlines, and who approves what." },
  { key: "processes", label: "Reconstructing processes", detail: "The steps your team already follows." },
  { key: "conflicts", label: "Checking conflicts", detail: "Where two documents say different things." },
  { key: "review", label: "Preparing review", detail: "Nothing is live until you approve it." },
] as const

const ENGINE_VERB: Record<Processor, string> = {
  "claude-code": "Reading happens on this machine.",
  codex: "Reading happens on this machine.",
  "cursor-agent": "Reading happens on this machine.",
  managed: "Reading happens for you.",
}

/** Build order: centre first, then outwards. */
const NODES: { x: number; y: number; hue: number }[] = [
  { x: 90, y: 88, hue: 175 },
  { x: 68, y: 62, hue: 195 },
  { x: 112, y: 58, hue: 155 },
  { x: 52, y: 96, hue: 210 },
  { x: 128, y: 92, hue: 140 },
  { x: 78, y: 118, hue: 25 },
  { x: 108, y: 120, hue: 320 },
  { x: 48, y: 52, hue: 230 },
  { x: 132, y: 48, hue: 125 },
  { x: 38, y: 78, hue: 250 },
  { x: 148, y: 76, hue: 95 },
  { x: 64, y: 142, hue: 15 },
  { x: 116, y: 146, hue: 300 },
  { x: 90, y: 42, hue: 185 },
  { x: 90, y: 156, hue: 340 },
  { x: 28, y: 118, hue: 265 },
  { x: 158, y: 118, hue: 75 },
  { x: 56, y: 28, hue: 205 },
  { x: 124, y: 26, hue: 160 },
  { x: 90, y: 18, hue: 145 },
]

const LINKS: [number, number][] = [
  [0, 1],
  [0, 2],
  [0, 3],
  [0, 4],
  [0, 5],
  [0, 6],
  [1, 7],
  [2, 8],
  [1, 9],
  [2, 10],
  [5, 11],
  [6, 12],
  [1, 13],
  [2, 13],
  [5, 14],
  [6, 14],
  [3, 15],
  [4, 16],
  [7, 17],
  [8, 18],
  [13, 19],
  [3, 5],
  [4, 6],
  [9, 15],
  [10, 16],
]

function hsl(hue: number, s = 62, l = 48, a = 1) {
  return `hsla(${hue}, ${s}%, ${l}%, ${a})`
}

export function StepBuilding({
  company,
  processor,
  onDone,
}: {
  company: string
  processor: Processor
  onDone: () => void
}) {
  const [work, setWork] = useState<Work | null>(null)
  const [found, setFound] = useState(0)
  const [releasing, setReleasing] = useState(false)
  const [openPhase, setOpenPhase] = useState<string | null>(null)
  const [feedOpen, setFeedOpen] = useState(true)
  const { refresh } = useApp()
  const done = useRef(onDone)
  const pull = useRef(refresh)
  useEffect(() => {
    done.current = onDone
    pull.current = refresh
  }, [onDone, refresh])

  useEffect(() => {
    let cancelled = false
    let seenWork = false
    let timer = 0

    const poll = async () => {
      const [next, health] = await Promise.all([getWorkFeed(), api.getWork()])
      if (cancelled) return

      if (next) setWork(next)
      setFound(health.objects)

      const moving =
        next !== null && (next.stage === "reading" || next.stage === "thinking" || next.stage === "preparing")
      const held = next?.stage === "held" || (next?.held?.count ?? 0) > 0
      if (moving || held || next?.done || health.objects > 0 || health.documents > 0) {
        seenWork = true
      }

      if (seenWork && next?.stage === "idle") {
        cancelled = true
        await pull.current()
        done.current()
        return
      }

      timer = window.setTimeout(() => void poll(), moving ? 1000 : 2500)
    }

    void poll()
    return () => {
      cancelled = true
      window.clearTimeout(timer)
    }
  }, [])

  const paused = work?.stage === "held"
  const moving = work !== null && !paused && work.stage !== "idle"
  const stage = work?.stage ?? null

  // Burst counters reset when settle/relate starts (e.g. "0 of 1"). That used
  // to slam the bar back to 0% and the checklist to "Reading company files"
  // after the documents were already in the brain — looks like a restart.
  const burstPercent =
    work && work.total > 0 ? Math.round((work.done / work.total) * 100) : stage === "idle" ? 100 : 0
  const stageFloor =
    stage === "idle" ? 100 : stage === "preparing" ? 88 : stage === "thinking" ? 75 : stage === "reading" ? 8 : 0
  const rawPercent =
    stage === "reading" ? Math.max(stageFloor, Math.round(8 + burstPercent * 0.67)) : Math.max(stageFloor, burstPercent)

  const [peakPercent, setPeakPercent] = useState(0)
  useEffect(() => {
    setPeakPercent((p) => Math.max(p, rawPercent, found > 0 ? 55 : 0))
  }, [rawPercent, found])
  const percent = stage === "idle" ? 100 : Math.max(peakPercent, rawPercent)

  const phase =
    stage === "preparing" || stage === "idle"
      ? PHASES.length - 1
      : stage === "thinking"
        ? PHASES.length - 2
        : stage === "held"
          ? Math.min(PHASES.length - 2, Math.max(0, Math.floor((percent / 100) * (PHASES.length - 1)) - 1))
          : Math.min(PHASES.length - 2, Math.floor((percent / 100) * (PHASES.length - 1)))

  const finished = stage === "idle" && percent >= 100
  const stalled = work !== null && stage === "idle" && work.total === 0 && found === 0 && work.done === 0
  const lines = work?.lines ?? []
  const workingLine = lines.find((line) => line.state === "working")
  const recent = lines.slice(0, 8)

  useEffect(() => {
    const current = PHASES[phase]?.key
    if (current && !paused) setOpenPhase(current)
  }, [phase, paused])

  const continueOnBattery = async () => {
    setReleasing(true)
    try {
      await background.setPolicy({
        processing: "automatic",
        pauseOnBattery: false,
        largeScan: 500,
        engine: processor,
      })
      await background.release()
    } finally {
      setReleasing(false)
    }
  }

  return (
    <div>
      <div className="text-center">
        <h1 className="text-[28px] font-semibold leading-tight tracking-[-0.024em] text-ink">
          Building {company}
        </h1>
        <p className="mx-auto mt-2.5 max-w-[40ch] text-[14px] leading-relaxed text-muted">
          The company brain forms as documents are read. {ENGINE_VERB[processor]}.
        </p>
      </div>

      {/* Hero: the brain grows step by step — this is what the client watches. */}
      <CompanyBrain
        progress={percent / 100}
        found={found}
        paused={Boolean(paused)}
        active={moving}
        company={company}
        phase={phase}
      />

      <div className="mx-auto mt-2 max-w-[520px]">
        <div className="flex items-center gap-3">
          <span className="h-1.5 flex-1 overflow-hidden rounded-full bg-surface-3">
            <span
              className={cn(
                "block h-full rounded-full transition-[width,background-color] duration-700 ease-out",
                paused ? "bg-pending" : "bg-accent",
              )}
              style={{ width: `${percent}%` }}
            />
          </span>
          <span className="tabular w-10 shrink-0 text-right text-[12.5px] text-muted">{percent}%</span>
        </div>
        <p className="tabular mt-2 text-center text-[12px] text-faint">
          {paused && work?.held
            ? `${formatCount(work.held.count)} waiting · ${formatCount(found)} found so far`
            : stage === "preparing" || stage === "thinking"
              ? `${formatCount(found)} in the brain · finishing up`
              : work && work.total > 0
                ? `${formatCount(work.done)} of ${formatCount(work.total)} · ${formatCount(found)} found so far`
                : `${formatCount(found)} found so far`}
        </p>
      </div>

      <div
        className={cn(
          "mx-auto mt-5 max-w-[560px] rounded-xl border px-4 py-3.5 transition-colors",
          paused
            ? "border-pending/30 bg-pending-soft"
            : moving
              ? "border-accent/30 bg-accent-soft"
              : "border-line bg-surface-2",
        )}
      >
        <div className="flex items-start gap-3">
          {paused ? (
            <PauseCircle className="mt-0.5 size-4 shrink-0 text-pending" />
          ) : moving ? (
            <Loader2 className="mt-0.5 size-4 shrink-0 animate-spin text-accent" />
          ) : (
            <Check className="mt-0.5 size-4 shrink-0 text-confirmed" />
          )}
          <div className="min-w-0 flex-1">
            <p className="text-[14px] font-medium text-ink">
              {paused && work?.held
                ? `Paused — ${work.held.reason}`
                : work?.doing ?? "Waiting for the first document…"}
            </p>
            {workingLine ? (
              <p className="mt-1 truncate text-[12.5px] text-muted">
                <span className="font-medium text-ink">{workingLine.subject}</span>
                {workingLine.note ? ` · ${workingLine.note}` : ""}
              </p>
            ) : null}
          </div>
        </div>
        {paused && work?.held ? (
          <Button
            size="sm"
            variant="primary"
            className="mt-3"
            disabled={releasing}
            onClick={() => void continueOnBattery()}
          >
            {releasing ? "Starting…" : "Continue reading on battery"}
          </Button>
        ) : null}
      </div>

      {stalled ? (
        <p className="mx-auto mt-4 flex max-w-[560px] items-start gap-2 text-[13px] text-pending">
          <AlertTriangle className="mt-[2px] size-4 shrink-0" />
          Nothing is queued. If this does not move, the background worker is not running —
          the status bar at the bottom says which.
        </p>
      ) : null}

      <div className="mx-auto mt-5 max-w-[560px] overflow-hidden rounded-xl border border-line bg-surface">
        <button
          type="button"
          onClick={() => setFeedOpen((v) => !v)}
          className="flex w-full items-center justify-between gap-3 px-4 py-3 text-left transition-colors hover:bg-surface-2"
        >
          <span className="text-[13px] font-medium text-ink">
            What has been read
            {recent.length > 0 ? <span className="ml-2 tabular font-normal text-faint">{recent.length}</span> : null}
          </span>
          <ChevronDown className={cn("size-4 text-faint transition-transform", feedOpen && "rotate-180")} />
        </button>
        {feedOpen ? (
          <ol className="scroll-thin max-h-[200px] overflow-y-auto border-t border-line">
            {recent.length === 0 ? (
              <li className="px-4 py-3 text-[12.5px] text-muted">
                Notes appear here as each document is read.
              </li>
            ) : (
              recent.map((line, index) => (
                <FeedLine key={`${line.at}-${line.subject}-${index}`} line={line} />
              ))
            )}
          </ol>
        ) : null}
      </div>

      <ul className="mx-auto mt-5 grid max-w-[560px] gap-0.5">
        {PHASES.map((item, index) => {
          const state =
            finished || index < phase ? "done" : index === phase ? (paused ? "paused" : "running") : "waiting"
          const open = openPhase === item.key && state !== "waiting"
          return (
            <li key={item.key}>
              <button
                type="button"
                disabled={state === "waiting"}
                onClick={() => setOpenPhase((cur) => (cur === item.key ? null : item.key))}
                className={cn(
                  "flex w-full gap-3 rounded-lg px-2.5 py-2 text-left transition-colors",
                  state === "waiting" ? "cursor-default" : "hover:bg-surface-2",
                  open && "bg-surface-2",
                )}
              >
                <span className="mt-0.5 shrink-0">
                  {state === "done" ? (
                    <Check className="size-[17px] text-confirmed" />
                  ) : state === "running" ? (
                    <Loader2 className="size-[17px] animate-spin text-accent" />
                  ) : state === "paused" ? (
                    <PauseCircle className="size-[17px] text-pending" />
                  ) : (
                    <span className="block size-[17px] rounded-full border border-line-strong" />
                  )}
                </span>
                <span className="min-w-0 flex-1">
                  <span
                    className={cn(
                      "flex items-center justify-between gap-2 text-[13.5px]",
                      state === "waiting" ? "text-faint" : "font-medium text-ink",
                    )}
                  >
                    {item.label}
                    {state !== "waiting" ? (
                      <ChevronDown
                        className={cn("size-3.5 shrink-0 text-faint transition-transform", open && "rotate-180")}
                      />
                    ) : null}
                  </span>
                  {open ? (
                    <span className="mt-1 block text-[12.5px] leading-relaxed text-muted">
                      {state === "running" && work?.doing
                        ? `${item.detail} Right now: ${work.doing.toLowerCase()}.`
                        : item.detail}
                    </span>
                  ) : null}
                </span>
              </button>
            </li>
          )
        })}
      </ul>

      <p className="mx-auto mt-8 max-w-[560px] text-center text-[12.5px] leading-relaxed text-faint">
        The company home opens after this finishes — not before.
      </p>
    </div>
  )
}

function FeedLine({ line }: { line: WorkLine }) {
  return (
    <li
      className={cn(
        "flex items-baseline gap-2.5 border-b border-line/60 px-4 py-2.5 text-[12.5px] last:border-b-0",
        line.state === "working" && "bg-accent-soft/60",
      )}
    >
      {line.state === "failed" ? (
        <AlertTriangle className="size-3 shrink-0 translate-y-[2px] text-conflict" />
      ) : line.state === "working" ? (
        <Loader2 className="size-3 shrink-0 translate-y-[2px] animate-spin text-accent" />
      ) : (
        <Check className="size-3 shrink-0 translate-y-[2px] text-confirmed" />
      )}
      <span className={cn("min-w-0 flex-1 truncate font-medium", line.state === "failed" ? "text-conflict" : "text-ink")}>
        {line.subject}
      </span>
      <span className="min-w-0 flex-[1.4] truncate text-muted">{line.note}</span>
      <span className="shrink-0 font-mono text-[11px] tabular-nums text-faint">{formatRelative(line.at)}</span>
    </li>
  )
}

/**
 * Company brain as the hero of first-run.
 *
 * A real brain silhouette draws itself, then coloured lobes and synapses
 * light up in order as progress rises. Growth is monotonic.
 */
function CompanyBrain({
  progress,
  found,
  paused,
  active,
  company,
  phase,
}: {
  progress: number
  found: number
  paused: boolean
  active: boolean
  company: string
  phase: number
}) {
  const target = useMemo(() => {
    // Objects already in the lake must light the brain even when a later
    // burst (relate / skills) reports 0% — otherwise refresh looks like a reboot.
    const fromFound =
      found <= 0 ? 0 : Math.min(0.82, 0.28 + Math.log10(found + 1) * 0.28)
    const fromBar = progress * 0.45
    const fromPhase = (phase / Math.max(1, PHASES.length - 1)) * 0.2
    return Math.min(1, Math.max(0.05, fromFound + fromBar + fromPhase))
  }, [progress, found, phase])

  const [grown, setGrown] = useState(0.05)
  useEffect(() => {
    setGrown((current) => Math.max(current, target))
  }, [target])

  const [shownNodes, setShownNodes] = useState(1)
  const wantNodes = Math.max(1, Math.round(grown * NODES.length))
  useEffect(() => {
    if (shownNodes >= wantNodes) return
    const id = window.setTimeout(() => {
      setShownNodes((n) => Math.min(wantNodes, n + 1))
    }, 110)
    return () => window.clearTimeout(id)
  }, [wantNodes, shownNodes])

  const shownLinks = LINKS.filter(([a, b]) => a < shownNodes && b < shownNodes)
  const lobeOpacity = Math.min(0.55, 0.12 + grown * 0.5)

  return (
    <div className="relative mx-auto mt-6 mb-1 w-full max-w-[360px]">
      <div
        className={cn(
          "relative aspect-square overflow-hidden rounded-[32px] border border-line",
          "bg-[radial-gradient(ellipse_at_50%_42%,#e8f7f4_0%,#f7f3ff_42%,#fff6ee_78%,#f4f6f7_100%)]",
          active && "brain-glow",
        )}
      >
        <svg viewBox="0 0 200 200" className="h-full w-full p-2" aria-hidden>
          <defs>
            <linearGradient id="lobe-left" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0%" stopColor="#5b8def" stopOpacity={lobeOpacity} />
              <stop offset="100%" stopColor="#37a8a0" stopOpacity={lobeOpacity * 0.55} />
            </linearGradient>
            <linearGradient id="lobe-right" x1="1" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="#e07a5f" stopOpacity={lobeOpacity} />
              <stop offset="55%" stopColor="#c77dff" stopOpacity={lobeOpacity * 0.7} />
              <stop offset="100%" stopColor="#37a8a0" stopOpacity={lobeOpacity * 0.45} />
            </linearGradient>
            <linearGradient id="lobe-stem" x1="0.5" y1="0" x2="0.5" y2="1">
              <stop offset="0%" stopColor="#f2cc8f" stopOpacity={lobeOpacity * 0.9} />
              <stop offset="100%" stopColor="#e07a5f" stopOpacity={lobeOpacity * 0.5} />
            </linearGradient>
            <linearGradient id="rim-draw" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0%" stopColor="#5b8def" />
              <stop offset="35%" stopColor="#37a8a0" />
              <stop offset="65%" stopColor="#c77dff" />
              <stop offset="100%" stopColor="#e07a5f" />
            </linearGradient>
            <filter id="soft-glow" x="-30%" y="-30%" width="160%" height="160%">
              <feGaussianBlur stdDeviation="1.4" result="b" />
              <feMerge>
                <feMergeNode in="b" />
                <feMergeNode in="SourceGraphic" />
              </feMerge>
            </filter>
          </defs>

          <g className="brain-lobes" style={{ opacity: lobeOpacity > 0.14 ? 1 : 0 }}>
            <path
              d="M100 38c-22-2-42 10-50 30-8 18-6 40 8 54 10 10 14 22 10 34 18 6 36 4 50-6V38Z"
              fill="url(#lobe-left)"
            />
            <path
              d="M100 38c22-2 42 10 50 30 8 18 6 40-8 54-10 10-14 22-10 34-18 6-36 4-50-6V38Z"
              fill="url(#lobe-right)"
            />
            <path
              d="M88 148c4 14 10 22 12 28 2-6 8-14 12-28-8 2-16 2-24 0Z"
              fill="url(#lobe-stem)"
            />
          </g>

          <path
            d="M100 28c18-8 40-4 52 12 10 12 14 28 12 44 14 6 24 22 20 40-4 16-18 28-34 30 0 10-4 20-14 28-8 6-18 8-26 4-8 4-18 2-26-4-10-8-14-18-14-28-16-2-30-14-34-30-4-18 6-34 20-40-2-16 2-32 12-44 12-16 34-20 52-12Z"
            fill="none"
            stroke="url(#rim-draw)"
            strokeWidth="2.2"
            strokeLinecap="round"
            strokeLinejoin="round"
            className="brain-outline brain-outline-on"
            pathLength={1}
            filter="url(#soft-glow)"
          />

          <g
            fill="none"
            stroke="url(#rim-draw)"
            strokeWidth="1.15"
            strokeLinecap="round"
            opacity={0.35 + grown * 0.4}
            className="brain-folds"
          >
            <path d="M58 70c18 8 28 6 40-4" className="brain-fold" style={{ animationDelay: "0.4s" }} pathLength={1} />
            <path d="M102 66c14 6 30 10 42 2" className="brain-fold" style={{ animationDelay: "0.55s" }} pathLength={1} />
            <path d="M52 100c16 4 26 14 34 28" className="brain-fold" style={{ animationDelay: "0.7s" }} pathLength={1} />
            <path d="M148 98c-14 6-24 16-32 30" className="brain-fold" style={{ animationDelay: "0.85s" }} pathLength={1} />
            <path d="M78 128c8 10 16 16 22 22" className="brain-fold" style={{ animationDelay: "1s" }} pathLength={1} />
            <path d="M122 128c-8 10-16 16-22 22" className="brain-fold" style={{ animationDelay: "1.1s" }} pathLength={1} />
          </g>

          {shownLinks.map(([a, b], i) => {
            const from = NODES[a]
            const to = NODES[b]
            const mid = (from.hue + to.hue) / 2
            return (
              <line
                key={`link-${a}-${b}`}
                x1={from.x + 10}
                y1={from.y + 10}
                x2={to.x + 10}
                y2={to.y + 10}
                stroke={paused ? "var(--k-pending)" : hsl(mid, 70, 52)}
                strokeWidth="1.35"
                strokeLinecap="round"
                className={cn("brain-link", active && "brain-link-live")}
                style={{ animationDelay: `${i * 35}ms` }}
              />
            )
          })}

          {NODES.slice(0, shownNodes).map((node, i) => (
            <g
              key={`node-${i}`}
              className="brain-node-pop"
              style={{ animationDelay: `${i * 25}ms` }}
              filter="url(#soft-glow)"
            >
              <circle
                cx={node.x + 10}
                cy={node.y + 10}
                r={i === 0 ? 6.2 : i < 4 ? 3.8 : 2.7}
                fill={paused ? "var(--k-pending)" : hsl(node.hue, 72, 52)}
              />
              <circle
                cx={node.x + 10}
                cy={node.y + 10}
                r={i === 0 ? 3 : i < 4 ? 1.6 : 1.1}
                fill="white"
                fillOpacity={0.55}
              />
              {i === 0 && active ? (
                <circle
                  cx={node.x + 10}
                  cy={node.y + 10}
                  r="11"
                  fill="none"
                  stroke={hsl(node.hue, 70, 55)}
                  strokeOpacity="0.45"
                  className="brain-core-ring"
                  style={{ transformOrigin: `${node.x + 10}px ${node.y + 10}px` }}
                />
              ) : null}
            </g>
          ))}
        </svg>
      </div>

      <p className="mt-3 text-center text-[13px] text-ink">
        <span className="font-semibold">{company}</span>
        <span className="mt-0.5 block text-[12px] text-muted">
          {found > 0
            ? `${formatCount(found)} in the company brain · ${shownNodes} of ${NODES.length} lit`
            : `Drawing the company brain · ${shownNodes} of ${NODES.length} lit`}
        </span>
      </p>

      <style>{`
        .brain-lobes { transition: opacity 1.2s ease; }
        .brain-outline {
          stroke-dasharray: 1;
          stroke-dashoffset: 1;
        }
        .brain-outline-on {
          animation: brain-draw 2.2s cubic-bezier(0.4, 0, 0.2, 1) forwards;
        }
        @keyframes brain-draw {
          to { stroke-dashoffset: 0; }
        }
        .brain-fold {
          stroke-dasharray: 1;
          stroke-dashoffset: 1;
          animation: brain-draw 1.4s ease forwards;
        }
        .brain-link {
          opacity: 0;
          animation: brain-link-in 0.6s ease forwards;
        }
        @keyframes brain-link-in {
          from { opacity: 0; }
          to { opacity: 0.75; }
        }
        .brain-link-live {
          animation: brain-link-in 0.6s ease forwards, brain-pulse-stroke 2.2s ease-in-out infinite;
        }
        @keyframes brain-pulse-stroke {
          0%, 100% { opacity: 0.4; }
          50% { opacity: 0.95; }
        }
        .brain-node-pop {
          transform-box: fill-box;
          transform-origin: center;
          animation: brain-node-in 0.5s cubic-bezier(0.22, 1.25, 0.36, 1) both;
        }
        @keyframes brain-node-in {
          from { opacity: 0; transform: scale(0.15); }
          to { opacity: 1; transform: scale(1); }
        }
        .brain-core-ring {
          animation: brain-ring 2s ease-out infinite;
        }
        @keyframes brain-ring {
          0% { transform: scale(0.65); opacity: 0.55; }
          100% { transform: scale(2.4); opacity: 0; }
        }
        .brain-glow {
          box-shadow:
            0 0 0 1px color-mix(in srgb, #37a8a0 20%, transparent),
            0 22px 48px -18px color-mix(in srgb, #5b8def 35%, transparent),
            0 12px 32px -16px color-mix(in srgb, #c77dff 28%, transparent);
        }
        @media (prefers-reduced-motion: reduce) {
          .brain-outline-on, .brain-fold { stroke-dashoffset: 0; animation: none; }
          .brain-link-live, .brain-core-ring, .brain-node-pop, .brain-link {
            animation: none !important;
            opacity: 1;
          }
        }
      `}</style>
    </div>
  )
}
