import { useCallback, useEffect, useState } from "react"
import { useNavigate } from "react-router-dom"
import { ArrowDown, ArrowRight, X } from "lucide-react"
import { Button } from "@/components/ui/button"
import { useApp } from "@/state/AppState"

const TOUR_KEY = "knowlith.productTour"

type Step = {
  id: string
  /** data-tour attribute on the target */
  target: string
  title: string
  body: string
  /** Where to navigate before highlighting (optional). */
  to?: string
}

/**
 * Walks the owner through what Knowlith is for — not a feature dump.
 *
 * Each step points an arrow at a real control. Skipping is always available;
 * finishing writes localStorage so it does not start again on its own.
 */
const STEPS: Step[] = [
  {
    id: "flow",
    target: "tour-home",
    title: "What Knowlith does",
    body: "It reads your business files, asks you to confirm what is true, then lets Claude, Codex or Cursor answer from that — with a quote behind every claim.",
    to: "/home",
  },
  {
    id: "review",
    target: "tour-review",
    title: "For review",
    body: "New findings wait here. Nothing reaches an AI assistant until you confirm, edit or discard it.",
    to: "/review",
  },
  {
    id: "knowledge",
    target: "tour-browse",
    title: "Company knowledge",
    body: "Rules (decisions), processes (how work is done), terms (your vocabulary) and AI skills (tasks assistants can run). Open any item to see the quote it came from.",
    to: "/browse",
  },
  {
    id: "brain",
    target: "tour-brain",
    title: "Company brain",
    body: "A live map of what you confirmed. Ask an AI about any node — it opens in your assistant with that knowledge named.",
    to: "/brain",
  },
  {
    id: "assistants",
    target: "tour-connect",
    title: "AI assistants",
    body: "Connect Claude Desktop, Codex / ChatGPT, Cursor Agent or Claude Code. Desktop apps open outside; CLIs open in Terminal.",
    to: "/connect",
  },
  {
    id: "try",
    target: "tour-ask-ai",
    title: "Try it",
    body: "From Home you can ask how well an AI knows your company. On any rule, process, term or skill, use “Try in your AI” — then History shows what it actually read.",
    to: "/home",
  },
]

export function startProductTour() {
  window.dispatchEvent(new Event("knowlith:start-tour"))
}

export function ProductTour() {
  const navigate = useNavigate()
  const { setFirstRun } = useApp()
  const [active, setActive] = useState(false)
  const [index, setIndex] = useState(0)
  const [rect, setRect] = useState<DOMRect | null>(null)

  const finish = useCallback(() => {
    setActive(false)
    localStorage.setItem(TOUR_KEY, "done")
    setFirstRun(null)
  }, [setFirstRun])

  const measure = useCallback(() => {
    const step = STEPS[index]
    if (!step) return
    const el = document.querySelector(`[data-tour="${step.target}"]`)
    setRect(el?.getBoundingClientRect() ?? null)
  }, [index])

  useEffect(() => {
    const onStart = () => {
      setIndex(0)
      setActive(true)
    }
    window.addEventListener("knowlith:start-tour", onStart)
    // After onboarding, or when the owner never finished.
    if (localStorage.getItem(TOUR_KEY) === "pending") {
      setActive(true)
    }
    return () => window.removeEventListener("knowlith:start-tour", onStart)
  }, [])

  useEffect(() => {
    if (!active) return
    const step = STEPS[index]
    if (step?.to) navigate(step.to)
    const t = window.setTimeout(measure, 80)
    window.addEventListener("resize", measure)
    window.addEventListener("scroll", measure, true)
    return () => {
      window.clearTimeout(t)
      window.removeEventListener("resize", measure)
      window.removeEventListener("scroll", measure, true)
    }
  }, [active, index, measure, navigate])

  if (!active) return null

  const step = STEPS[index]
  const last = index >= STEPS.length - 1
  const cardTop = rect ? Math.min(window.innerHeight - 200, rect.bottom + 16) : 120
  const cardLeft = rect ? Math.max(16, Math.min(rect.left, window.innerWidth - 340)) : 24

  return (
    <div className="fixed inset-0 z-[60]" role="dialog" aria-modal="true" aria-label="Product tour">
      <div className="absolute inset-0 bg-black/45" onClick={finish} />
      {rect ? (
        <>
          <div
            className="pointer-events-none absolute rounded-lg ring-2 ring-accent ring-offset-2 ring-offset-transparent"
            style={{
              top: rect.top - 4,
              left: rect.left - 4,
              width: rect.width + 8,
              height: rect.height + 8,
            }}
          />
          <ArrowDown
            className="pointer-events-none absolute size-7 text-accent drop-shadow"
            style={{
              top: rect.bottom + 2,
              left: rect.left + rect.width / 2 - 14,
            }}
            aria-hidden
          />
        </>
      ) : null}

      <div
        className="absolute w-[min(320px,calc(100vw-32px))] rounded-xl border border-line bg-surface p-4 shadow-lg"
        style={{ top: cardTop, left: cardLeft }}
      >
        <div className="flex items-start justify-between gap-2">
          <div>
            <div className="text-[11px] font-medium uppercase tracking-wide text-faint">
              {index + 1} of {STEPS.length}
            </div>
            <h2 className="mt-1 text-[15px] font-semibold text-ink">{step.title}</h2>
          </div>
          <button type="button" onClick={finish} className="text-faint hover:text-ink" aria-label="Close tour">
            <X className="size-4" />
          </button>
        </div>
        <p className="mt-2 text-[13px] leading-relaxed text-muted">{step.body}</p>
        <div className="mt-4 flex items-center justify-between gap-2">
          <Button variant="ghost" size="sm" onClick={finish}>
            Skip
          </Button>
          <Button
            variant="primary"
            size="sm"
            onClick={() => {
              if (last) finish()
              else setIndex((i) => i + 1)
            }}
          >
            {last ? "Done" : "Next"}
            {!last ? <ArrowRight className="size-3.5" /> : null}
          </Button>
        </div>
      </div>
    </div>
  )
}

/** Mark that the tour should run once the shell mounts (after onboarding). */
export function scheduleProductTour() {
  localStorage.setItem(TOUR_KEY, "pending")
}
