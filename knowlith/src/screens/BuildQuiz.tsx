import { useCallback, useEffect, useState } from "react"
import { useNavigate } from "react-router-dom"
import { Check, Loader2, PartyPopper, X } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Panel } from "@/components/ui/surface"
import { api } from "@/lib/api"
import type { BuildQuiz as BuildQuizType } from "@/lib/types"
import { cn } from "@/lib/utils"
import { useApp } from "@/state/AppState"

export function BuildQuiz() {
  const navigate = useNavigate()
  const { refresh, buildStatus } = useApp()
  const [quiz, setQuiz] = useState<BuildQuizType | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [confirmed, setConfirmed] = useState<Record<string, boolean>>({})
  const [submitting, setSubmitting] = useState(false)
  const [submitError, setSubmitError] = useState<string | null>(null)

  const loadQuiz = useCallback(async () => {
    try {
      const q = await api.getBuildQuiz()
      setQuiz(q)
      if (q?.questions) {
        const all: Record<string, boolean> = {}
        for (const item of q.questions) {
          all[item.id] = true
        }
        setConfirmed(all)
      }
      setError(null)
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not load quiz")
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    void loadQuiz()
  }, [loadQuiz])

  // Supervisor can finish while this tab stays open; poll so the screen
  // catches up without a manual refresh.
  useEffect(() => {
    if (!buildStatus.quizPending && quiz?.state === "confirmed") return
    const timer = window.setInterval(() => {
      void loadQuiz()
    }, 4000)
    return () => window.clearInterval(timer)
  }, [buildStatus.quizPending, loadQuiz, quiz?.state])

  useEffect(() => {
    const onVisible = () => {
      if (document.visibilityState === "visible") void loadQuiz()
    }
    document.addEventListener("visibilitychange", onVisible)
    return () => document.removeEventListener("visibilitychange", onVisible)
  }, [loadQuiz])

  const submit = async () => {
    if (!quiz || quiz.state === "confirmed") return
    setSubmitting(true)
    setSubmitError(null)
    const approved = quiz.questions
      .filter((q) => confirmed[q.id])
      .flatMap((q) => (q.proposedObjectId ? [q.proposedObjectId] : []))
    const ok = await api.confirmBuildQuiz(quiz.id, approved)
    setSubmitting(false)
    if (!ok) {
      setSubmitError("Could not save your answers. Is Knowlith still running?")
      return
    }
    await Promise.all([refresh(), loadQuiz()])
  }

  if (loading) {
    return <div className="p-8 text-muted">Loading confirmation quiz…</div>
  }

  if (error || !quiz || quiz.questions.length === 0) {
    return (
      <div className="grid min-h-full place-items-center px-4 py-16">
        <div className="max-w-[42ch] text-center">
          <h1 className="text-[17px] font-semibold text-ink">No build quiz yet</h1>
          <p className="mt-1.5 text-[13px] text-muted">
            {error ?? "The supervisor has not finished synthesising company knowledge."}
          </p>
          <Button className="mt-4" variant="primary" onClick={() => navigate("/home")}>
            Back to Home
          </Button>
        </div>
      </div>
    )
  }

  if (quiz.state === "confirmed") {
    const yes = quiz.questions.filter((q) => confirmed[q.id]).length
    return (
      <div className="grid min-h-full place-items-center px-4 py-16">
        <div className="max-w-[46ch] text-center">
          <PartyPopper className="mx-auto size-6 text-confirmed" />
          <h1 className="mt-3 text-[18px] font-semibold text-ink">Build confirmed</h1>
          <p className="mt-2 text-[13px] leading-relaxed text-muted">
            {yes} of {quiz.questions.length} answers are in company knowledge. Anything marked &ldquo;Not
            quite&rdquo; was left out.
          </p>
          <div className="mt-6 flex flex-wrap items-center justify-center gap-2">
            <Button variant="primary" onClick={() => navigate("/review")}>
              For review
            </Button>
            <Button variant="ghost" onClick={() => navigate("/home")}>
              Home
            </Button>
          </div>
        </div>
      </div>
    )
  }

  const yesCount = quiz.questions.filter((q) => confirmed[q.id]).length

  return (
    <div className="mx-auto w-full max-w-[720px] px-5 py-8">
      <h1 className="text-[22px] font-semibold tracking-[-0.02em] text-ink">Confirm what we learned</h1>
      <p className="mt-2 text-[14px] leading-relaxed text-muted">
        The build supervisor read your folder as a whole. Mark each answer, then press{" "}
        <span className="font-medium text-ink">Confirm and finish build</span> — nothing is saved until then.
      </p>
      <p className="mt-1 text-[12.5px] text-faint">
        {yesCount} of {quiz.questions.length} marked correct · changes apply only after you confirm
      </p>

      <div className="mt-6 grid gap-3">
        {quiz.questions.map((q) => {
          const yes = confirmed[q.id] !== false
          return (
            <Panel
              key={q.id}
              className={cn(
                "p-4 transition-colors",
                yes ? "border-confirmed/25" : "border-conflict/25",
              )}
            >
              <div className="text-[14px] font-medium text-ink">{q.question}</div>
              <p className="mt-2 text-[13px] leading-relaxed text-muted">{q.agentAnswer}</p>
              {q.evidence.map((e) => (
                <blockquote
                  key={`${e.documentId}-${e.quote.slice(0, 24)}`}
                  className="mt-3 border-l-2 border-line pl-3 text-[12px] text-muted"
                >
                  <span className="font-medium text-ink">{e.documentName}</span>
                  <p className="mt-1 leading-relaxed">&ldquo;{e.quote}&rdquo;</p>
                </blockquote>
              ))}
              <div className="mt-3 flex flex-wrap gap-2">
                <Button
                  type="button"
                  size="sm"
                  variant="ghost"
                  className={cn(
                    yes &&
                      "border border-confirmed/40 bg-confirmed-soft text-confirmed hover:bg-confirmed-soft hover:text-confirmed",
                  )}
                  onClick={() => setConfirmed((c) => ({ ...c, [q.id]: true }))}
                >
                  <Check />
                  Correct
                </Button>
                <Button
                  type="button"
                  size="sm"
                  variant="ghost"
                  className={cn(
                    !yes &&
                      "border border-conflict/40 bg-conflict-soft text-conflict hover:bg-conflict-soft hover:text-conflict",
                  )}
                  onClick={() => setConfirmed((c) => ({ ...c, [q.id]: false }))}
                >
                  <X />
                  Not quite
                </Button>
              </div>
            </Panel>
          )
        })}
      </div>

      {submitError ? <p className="mt-4 text-[13px] text-conflict">{submitError}</p> : null}

      <div className="mt-8 flex flex-wrap gap-3">
        <Button variant="primary" size="lg" disabled={submitting} onClick={() => void submit()}>
          {submitting ? <Loader2 className="size-4 animate-spin" /> : null}
          Confirm and finish build
        </Button>
        <Button variant="ghost" size="lg" disabled={submitting} onClick={() => navigate("/review")}>
          Review details first
        </Button>
      </div>
    </div>
  )
}
