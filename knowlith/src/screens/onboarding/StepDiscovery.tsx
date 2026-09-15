import { ArrowRight, GitMerge, Percent, Workflow } from "lucide-react"
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
  const { discovery, review } = useApp()

  // "Termoval d.o.o." already ends in a full stop, and "d.o.o.." reads as a
  // typo in the one sentence the owner is most likely to screenshot.
  const sentence = company.endsWith(".") ? company : `${company}.`

  const counts: { value: number; label: string }[] = [
    { value: discovery?.rules ?? 12, label: "Rules" },
    { value: discovery?.processes ?? 4, label: "Processes" },
    { value: discovery?.terms ?? 37, label: "Company terms" },
  ]

  // Skills are built from processes the owner approves, so at this moment
  // there are none and a placeholder count would be the one number on this
  // screen that is not true.
  if (discovery?.skills) {
    counts.push({ value: discovery.skills, label: "Skills" })
  }

  const findings = [
    {
      Icon: Percent,
      title: "We found how discounts are approved",
      detail: "Two limits, and who can go past them.",
    },
    {
      Icon: Workflow,
      title: "We found your customer quotation process",
      detail: "Six steps, reconstructed from how your offers are actually written.",
    },
    {
      Icon: GitMerge,
      title: "Two documents disagree about service response time",
      detail: "You decide which one is current.",
      conflict: true,
    },
  ]

  return (
    <div>
      <h1 className="text-[28px] font-semibold leading-tight tracking-[-0.024em] text-ink">
        We started understanding {sentence}
      </h1>
      <p className="mt-2.5 text-[14px] leading-relaxed text-muted">
        Here is the first draft. None of it reaches your AI tools until you approve it.
      </p>

      {/* The column count follows the tiles. A fixed four leaves an empty
          cell whose background is the divider colour, which reads as a
          missing number rather than as a number we do not have. */}
      <div
        className={`mt-8 grid grid-cols-2 gap-px overflow-hidden rounded-xl border border-line bg-line ${
          counts.length === 4 ? "sm:grid-cols-4" : "sm:grid-cols-3"
        }`}
      >
        {counts.map((count) => (
          <div key={count.label} className="bg-surface px-4 py-4">
            <div className="tabular text-[26px] font-semibold leading-none text-ink">{count.value}</div>
            <div className="mt-1.5 text-[12px] text-muted">{count.label}</div>
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
          {review.length || 7} items are waiting. It takes about a minute each.
        </p>
      </div>
    </div>
  )
}
