import { useState } from "react"
import { useNavigate } from "react-router-dom"
import { ArrowLeft, ArrowRight } from "lucide-react"
import { Button } from "@/components/ui/button"
import { demoInventory, type Inventory } from "@/lib/inventory"
import type { Processor, SourceKind } from "@/lib/types"
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
 */
const DECISIONS = 4

export function Onboarding() {
  const navigate = useNavigate()
  const { completeOnboarding, setFirstRun, setCompany } = useApp()

  const [step, setStep] = useState(0)
  const [name, setName] = useState("")
  const [logo, setLogo] = useState<string | null>(null)
  const [kind, setKind] = useState<SourceKind | null>(null)
  const [path, setPath] = useState("")
  const [inventory, setInventory] = useState<Inventory | null>(null)
  const [processor, setProcessor] = useState<Processor>("codex")
  const [allowStart, setAllowStart] = useState(false)
  const [granted, setGranted] = useState(false)

  const canContinue =
    (step === 0 && name.trim().length > 1) ||
    (step === 1 && inventory !== null) ||
    (step === 2 && granted) ||
    (step === 3 && (processor === "managed" || allowStart))

  const finishWizard = () => {
    setCompany(name.trim(), logo)
    completeOnboarding()
  }

  const wide = step >= 4

  return (
    <div className="min-h-full bg-surface">
      <div className={cn("mx-auto w-full px-5 py-12 sm:py-16", wide ? "max-w-[720px]" : "max-w-[600px]")}>
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

        {step === 4 ? (
          <StepPreview
            inventory={inventory ?? demoInventory}
            path={path}
            onBuild={() => setStep(5)}
          />
        ) : null}

        {step === 5 ? (
          <StepBuilding
            company={name.trim() || "your company"}
            onDone={() => setStep(6)}
            onLeave={() => {
              finishWizard()
              navigate("/home")
            }}
          />
        ) : null}

        {step === 6 ? (
          <StepDiscovery
            company={name.trim() || "your company"}
            onReview={() => {
              finishWizard()
              setFirstRun("review")
              navigate("/review")
            }}
          />
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

        {step < 5 ? (
          <button
            type="button"
            onClick={() => {
              finishWizard()
              navigate("/home")
            }}
            className="mt-10 text-[12px] text-faint underline-offset-4 transition-colors hover:text-muted hover:underline"
          >
            Skip and look around first
          </button>
        ) : null}
      </div>
    </div>
  )
}
