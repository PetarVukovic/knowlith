import { useEffect, useState } from "react"
import { useNavigate } from "react-router-dom"
import { ArrowLeft, ArrowRight } from "lucide-react"
import { Button } from "@/components/ui/button"
import { background, getWorkFeed } from "@/lib/api"
import type { Inventory, Processor, SourceKind } from "@/lib/types"
import { cn } from "@/lib/utils"
import { useApp } from "@/state/AppState"
import { StepAccess } from "./StepAccess"
import { StepBuilding } from "./StepBuilding"
import { StepDiscovery } from "./StepDiscovery"
import { StepPreview } from "./StepPreview"
import { StepProcessing } from "./StepProcessing"
import { StepSource } from "./StepSource"
import { StepWelcome } from "./StepWelcome"

/**
 * The first few minutes.
 *
 * One decision per screen, in the order that earns the next one: say what this
 * is, get a folder, say exactly what will happen to it, choose who reads it,
 * show what is in there before touching it, then read it. The owner reaches
 * their first approved rule before meeting a single technical idea — no
 * connectors, no configuration, no vocabulary they would have to learn.
 *
 * Home stays closed until the folder has been read and the first-run path
 * (review → connect) has finished. Escaping early used to open an empty
 * dashboard and look like the product had nothing to say.
 *
 * The wizard step itself is not stored. Refresh used to drop the owner on
 * Welcome while the daemon was still reading — the lake already had a source,
 * and the honest screen is Building or Discovery, not the start.
 */
const DECISIONS = 4

function readFreshFlag(): boolean {
  try {
    return localStorage.getItem("knowlith.setupFresh") === "yes"
  } catch {
    return false
  }
}

function clearFreshFlag() {
  try {
    localStorage.removeItem("knowlith.setupFresh")
  } catch {
    /* private window */
  }
}

function asProcessor(value: string | undefined, fallback: Processor): Processor {
  if (value === "codex" || value === "claude-code" || value === "cursor-agent" || value === "managed") {
    return value
  }
  return fallback
}

export function Onboarding() {
  const navigate = useNavigate()
  const {
    completeOnboarding,
    setFirstRun,
    setCompany,
    addSource,
    ready,
    sources,
    companyName,
    companyLogo,
    discovery,
    objects,
  } = useApp()

  const [step, setStep] = useState(0)
  const [name, setName] = useState("")
  const [logo, setLogo] = useState<string | null>(null)
  const [kind, setKind] = useState<SourceKind | null>(null)
  const [path, setPath] = useState("")
  const [inventory, setInventory] = useState<Inventory | null>(null)
  const [processor, setProcessor] = useState<Processor>("codex")
  const [allowStart, setAllowStart] = useState(false)
  const [granted, setGranted] = useState(false)
  const [addError, setAddError] = useState<string | null>(null)
  /** False until we know whether to resume mid-read or start at Welcome. */
  const [placed, setPlaced] = useState(false)

  useEffect(() => {
    if (!ready || placed) return
    let cancelled = false

    const place = async () => {
      // Settings can send the owner here on purpose — do not resume mid-read.
      if (readFreshFlag()) {
        clearFreshFlag()
        const knownName =
          companyName.trim() && companyName !== "Your company" ? companyName.trim() : ""
        if (knownName) setName(knownName)
        if (companyLogo) setLogo(companyLogo)
        if (!cancelled) {
          setStep(0)
          setPlaced(true)
        }
        return
      }

      // No folder attached yet — the welcome path is the only honest one.
      if (sources.length === 0) {
        if (!cancelled) setPlaced(true)
        return
      }

      const [feed, policy] = await Promise.all([getWorkFeed(), background.policy()])
      if (cancelled) return

      const knownName = companyName.trim() && companyName !== "Your company" ? companyName.trim() : ""
      if (knownName) setName(knownName)
      if (companyLogo) setLogo(companyLogo)
      setProcessor(asProcessor(policy?.policy.engine ?? sources[0]?.processor, "codex"))

      const knowledge =
        (discovery?.rules ?? 0) +
        (discovery?.processes ?? 0) +
        (discovery?.terms ?? 0) +
        (discovery?.skills ?? 0)
      const busy = feed !== null && feed.stage !== "idle"
      // Still reading, held, or finished with nothing to show yet → Building.
      // Idle with findings → Discovery. Never Welcome once a source exists.
      setStep(busy || (knowledge === 0 && objects.length === 0) ? 5 : 6)
      setPlaced(true)
    }

    void place()
    return () => {
      cancelled = true
    }
  }, [ready, placed, sources, companyName, companyLogo, discovery, objects])

  const canContinue =
    (step === 0 && name.trim().length > 1) ||
    (step === 1 && inventory !== null) ||
    (step === 2 && granted) ||
    (step === 3 && (processor === "managed" || allowStart))

  /** Enter the real app only after the first read has produced something. */
  const enterApp = (next: "review") => {
    setCompany(name.trim(), logo)
    completeOnboarding()
    setFirstRun(next)
    navigate(`/${next}`)
  }

  /**
   * The moment the folder stops being a preview and becomes a source.
   *
   * Deliberately here and not on the screen before it: everything up to this
   * point has only counted file names, and the owner is entitled to reach
   * this button and still walk away having had nothing opened.
   */
  const build = async () => {
    setAddError(null)
    // The company is named now rather than at the end, so the daemon has it
    // before the first document is read and the status bar stops saying
    // "Your company" while the work is already running.
    setCompany(name.trim(), logo)
    // Persist who should read — the worker binds this at daemon start.
    // First read runs even on battery. pauseOnBattery:true here used to hold
    // every compile job, so the building screen hit 100% with nothing found
    // and spun on "Preparing review" until the laptop was plugged in.
    await background.setPolicy({
      processing: "automatic",
      pauseOnBattery: false,
      largeScan: 500,
      engine: processor,
    })
    const result = await addSource(path, undefined, processor)
    if (typeof result === "string") return setAddError(result)
    setStep(5)
  }

  const wide = step >= 4
  const displayName = name.trim() || (companyName !== "Your company" ? companyName : "") || "your company"

  if (!ready || !placed) {
    return <div className="min-h-full bg-surface" />
  }

  return (
    <div className="min-h-full bg-surface">
      <div
        className={cn(
          "mx-auto w-full px-5 py-12 sm:py-16",
          step >= 5 ? "max-w-[860px]" : wide ? "max-w-[720px]" : "max-w-[600px]",
        )}
      >
        {step < DECISIONS ? (
          <div className="flex items-center gap-1.5 pb-10" aria-hidden>
            {Array.from({ length: DECISIONS }, (_, i) => (
              <span
                key={i}
                className={cn(
                  "h-[3px] flex-1 rounded-full transition-colors duration-300",
                  i < step ? "bg-accent" : i === step ? "bg-accent/50" : "bg-line",
                )}
              />
            ))}
          </div>
        ) : null}

        {step === 0 ? (
          <StepWelcome name={name} onName={setName} logo={logo} onLogo={setLogo} />
        ) : null}

        {step === 1 ? (
          <StepSource
            kind={kind}
            onKind={(k) => {
              setKind(k)
              setInventory(null)
            }}
            onPick={(pickedKind, pickedPath, picked) => {
              setKind(pickedKind)
              setPath(pickedPath)
              setInventory(picked)
            }}
            inventory={inventory}
            path={path}
          />
        ) : null}

        {step === 2 ? (
          <StepAccess path={path} granted={granted} onGrant={setGranted} />
        ) : null}

        {step === 3 ? (
          <StepProcessing
            value={processor}
            onChange={setProcessor}
            allowStart={allowStart}
            onAllowStart={setAllowStart}
          />
        ) : null}

        {step === 4 && inventory ? (
          <>
            <StepPreview inventory={inventory} path={path} onBuild={() => void build()} />
            {addError ? <p className="mt-4 text-[13px] text-conflict">{addError}</p> : null}
          </>
        ) : null}

        {step === 5 ? (
          <StepBuilding
            company={displayName}
            processor={processor}
            onDone={() => setStep(6)}
          />
        ) : null}

        {step === 6 ? (
          <StepDiscovery company={displayName} onReview={() => enterApp("review")} />
        ) : null}

        {step < DECISIONS ? (
          <div className="mt-10 flex items-center justify-between gap-3">
            <Button
              variant="ghost"
              onClick={() => setStep((s) => Math.max(0, s - 1))}
              className={cn(step === 0 && "invisible")}
            >
              <ArrowLeft />
              Back
            </Button>
            <Button size="lg" variant="primary" disabled={!canContinue} onClick={() => setStep((s) => s + 1)}>
              Continue
              <ArrowRight />
            </Button>
          </div>
        ) : null}
      </div>
    </div>
  )
}
