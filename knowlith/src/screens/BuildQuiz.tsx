import { useEffect, useState } from "react"
import { useNavigate } from "react-router-dom"
import { Check, X } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Panel } from "@/components/ui/surface"
import { api } from "@/lib/api"
import type { BuildQuiz as BuildQuizType } from "@/lib/types"

export function BuildQuiz() {
  const navigate = useNavigate()
  const [quiz, setQuiz] = useState<BuildQuizType | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [confirmed, setConfirmed] = useState<Record<string, boolean>>({})

  useEffect(() => {
    void (async () => {
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
      } catch (e) {
        setError(e instanceof Error ? e.message : "Could not load quiz")
      } finally {
        setLoading(false)
      }
    })()
  }, [])

  const submit = async () => {
    if (!quiz) return
    const approved = quiz.questions
      .filter((q) => confirmed[q.id])
      .flatMap((q) => (q.proposedObjectId ? [q.proposedObjectId] : []))
    await api.confirmBuildQuiz(quiz.id, approved)
    navigate("/home")
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

  return (
    <div className="mx-auto w-full max-w-[720px] px-5 py-8">
      <h1 className="text-[22px] font-semibold tracking-[-0.02em] text-ink">Confirm what we learned</h1>
      <p className="mt-2 text-[14px] leading-relaxed text-muted">
        The build supervisor read your folder as a whole. Check each answer before it becomes company knowledge.
      </p>
      <div className="mt-6 grid gap-3">
        {quiz.questions.map((q) => (
          <Panel key={q.id} className="p-4">
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
            <div className="mt-3 flex gap-2">
              <Button
                size="sm"
                variant={confirmed[q.id] ? "primary" : "ghost"}
                onClick={() => setConfirmed((c) => ({ ...c, [q.id]: true }))}
              >
                <Check />
                Correct
              </Button>
              <Button
                size="sm"
                variant={!confirmed[q.id] ? "primary" : "ghost"}
                onClick={() => setConfirmed((c) => ({ ...c, [q.id]: false }))}
              >
                <X />
                Not quite
              </Button>
            </div>
          </Panel>
        ))}
      </div>
      <div className="mt-8 flex gap-3">
        <Button variant="primary" size="lg" onClick={() => void submit()}>
          Confirm and finish build
        </Button>
        <Button variant="ghost" size="lg" onClick={() => navigate("/review")}>
          Review details first
        </Button>
      </div>
    </div>
  )
}
