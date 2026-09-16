import { ArrowRight, BookOpen, GitMerge, Percent, Workflow } from "lucide-react"
import { kindMeta } from "@/components/Domain"
import { Button } from "@/components/ui/button"
import { useApp } from "@/state/AppState"

/**
 * The moment the product has to earn the next ten minutes.
 *
 * Counts alone do not do it — a number is a claim about volume, not about
 * understanding. The three cards underneath are the actual argument: Knowlith
 * names three things about this company that nobody typed in.
 */
export function StepDiscovery({ company, onReview }: { company: string; onReview: () => void }) {
  const { discovery, review, objects, mergeHints } = useApp()

  // "Termoval d.o.o." already ends in a full stop, and "d.o.o.." reads as a
  // typo in the one sentence the owner is most likely to screenshot.
  const sentence = company.endsWith(".") ? company : `${company}.`

  const counts: { value: number; label: string; meaning: string }[] = [
    { value: discovery?.rules ?? 0, label: kindMeta("rule").plural, meaning: kindMeta("rule").meaning },
    {
      value: discovery?.processes ?? 0,
      label: kindMeta("process").plural,
      meaning: kindMeta("process").meaning,
    },
    {
      value: discovery?.terms ?? 0,
      label: kindMeta("term").plural,
      meaning: kindMeta("term").meaning,
    },
  ]

  // Skills are built from processes the owner approves, so at this moment
  // there are none and a placeholder count would be the one number on this
  // screen that is not true.
  if (discovery?.skills) {
    counts.push({
      value: discovery.skills,
      label: kindMeta("skill").plural,
      meaning: kindMeta("skill").meaning,
    })
  }

  // Named out of what was actually read. The point of this screen is that
  // Knowlith can say three things about this company that nobody typed in,
  // and a card describing somebody else's company makes the opposite point.
  const first = (kind: string) => objects.find((o) => o.kind === kind)
  const rule = first("rule")
  const process = first("process")
  const term = first("term")
  // Only a real disagreement earns this card. A duplicate is two documents
  // saying the same thing, and calling that a conflict is the one claim this
  // screen must never make.
  const conflict = mergeHints.find((h) => h.kind === "disagreement")

  const findings = [
    rule
      ? { Icon: Percent, title: rule.title, detail: `From ${rule.evidence[0]?.documentName ?? "your documents"}.` }
      : null,
    process
      ? {
          Icon: Workflow,
          title: process.title,
          detail: "Reconstructed from how your own documents describe it.",
        }
      : null,
    conflict
      ? {
          Icon: GitMerge,
          title: `Two documents disagree about ${conflict.keepTitle.toLowerCase()}`,
          detail: "You decide which one is current.",
          conflict: true,
        }
      : term
        ? { Icon: BookOpen, title: term.title, detail: "A word your company uses in its own way." }
        : null,
  ].filter((x) => x !== null)

  return (
    <div>
      <h1 className="text-[28px] font-semibold leading-tight tracking-[-0.024em] text-ink">
        We started understanding {sentence}
      </h1>
      <p className="mt-2.5 text-[14px] leading-relaxed text-muted">
        Here is the first draft. None of it reaches your AI tools until you approve it.
      </p>

      <div className="mt-4 rounded-xl border border-line bg-surface-2 px-4 py-3 text-[12.5px] leading-relaxed text-muted">
        <strong className="font-medium text-ink">What the numbers mean.</strong>{" "}
        {kindMeta("rule").meaning} {kindMeta("process").meaning} {kindMeta("term").meaning}
      </div>

      {/* The column count follows the tiles. A fixed four leaves an empty
          cell whose background is the divider colour, which reads as a
          missing number rather than as a number we do not have. */}
      <div
        className={`mt-6 grid grid-cols-2 gap-px overflow-hidden rounded-xl border border-line bg-line ${
          counts.length === 4 ? "sm:grid-cols-4" : "sm:grid-cols-3"
        }`}
      >
        {counts.map((count) => (
          <div key={count.label} className="bg-surface px-4 py-4">
            <div className="tabular text-[26px] font-semibold leading-none text-ink">{count.value}</div>
            <div className="mt-1.5 text-[12px] font-medium text-ink">{count.label}</div>
            <div className="mt-1 text-[11px] leading-snug text-faint">{count.meaning}</div>
          </div>
        ))}
      </div>

      <div className="mt-6 grid gap-2.5">
        {findings.map((finding) => (
          <div
            key={finding.title}
            className="flex gap-3.5 rounded-xl border border-line bg-surface p-4"
          >
            <span
              className={
                finding.conflict
                  ? "mt-0.5 grid size-8 shrink-0 place-items-center rounded-lg bg-conflict-soft text-conflict"
                  : "mt-0.5 grid size-8 shrink-0 place-items-center rounded-lg bg-accent-soft text-accent"
              }
            >
              <finding.Icon className="size-4" />
            </span>
            <span className="min-w-0">
              <span className="block text-[14px] font-medium text-ink">{finding.title}</span>
              <span className="mt-0.5 block text-[12.5px] leading-relaxed text-muted">{finding.detail}</span>
            </span>
          </div>
        ))}
      </div>

      <div className="mt-9">
        <Button size="lg" variant="primary" onClick={onReview}>
          Review first discovery
          <ArrowRight />
        </Button>
        <p className="mt-2.5 text-[12px] text-faint">
          {review.length} {review.length === 1 ? "item is" : "items are"} waiting. It takes about a
          minute each.
        </p>
      </div>
    </div>
  )
}
