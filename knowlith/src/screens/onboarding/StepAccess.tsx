import { Ban, Eye, FileLock2, ShieldCheck } from "lucide-react"

/**
 * The trust screen.
 *
 * Read-only is the single most important promise this product makes, and a
 * sentence in a settings page nobody opens is not making it. The two columns
 * are deliberate: what Knowlith does and what it will never do, side by side,
 * so the second is as easy to find as the first.
 */
const WILL = [
  { Icon: Eye, text: "Open your documents and read their text." },
  { Icon: FileLock2, text: "Remember which sentence each answer came from." },
  { Icon: ShieldCheck, text: "Hold everything it finds until a person approves it." },
]

const WILL_NOT = [
  "Modify, rename, move or delete a single file.",
  "Write anything into this folder.",
  "Look outside the folder you chose.",
  "Share anything with other companies.",
]

export function StepAccess({
  path,
  granted,
  onGrant,
}: {
  path: string
  granted: boolean
  onGrant: (v: boolean) => void
}) {
  return (
    <div>
      <h1 className="text-[26px] font-semibold leading-tight tracking-[-0.022em] text-ink">
        What Knowlith does with your files
      </h1>
      <p className="mt-2.5 text-[14px] leading-relaxed text-muted">
        Knowlith will only read files inside{" "}
        <span className="font-medium text-ink">{path || "the folder you chose"}</span>. It will never modify or
        delete them.
      </p>

      <div className="mt-8 grid gap-4 sm:grid-cols-2">
        <div className="rounded-xl border border-line bg-surface p-4">
          <div className="label-xs mb-3 text-confirmed">What it does</div>
          <ul className="grid gap-3">
            {WILL.map((item) => (
              <li key={item.text} className="flex gap-2.5 text-[13px] leading-relaxed text-ink">
                <item.Icon className="mt-0.5 size-4 shrink-0 text-confirmed" />
                {item.text}
              </li>
            ))}
          </ul>
        </div>

        <div className="rounded-xl border border-line bg-surface p-4">
          <div className="label-xs mb-3 text-muted">What it never does</div>
          <ul className="grid gap-3">
            {WILL_NOT.map((text) => (
              <li key={text} className="flex gap-2.5 text-[13px] leading-relaxed text-muted">
                <Ban className="mt-0.5 size-4 shrink-0 text-faint" />
                {text}
              </li>
            ))}
          </ul>
        </div>
      </div>

      <label className="mt-6 flex cursor-pointer items-start gap-3 rounded-xl border border-line bg-surface-2 p-4 transition-colors hover:border-line-strong">
        <input
          type="checkbox"
          checked={granted}
          onChange={(e) => onGrant(e.target.checked)}
          className="mt-0.5 size-[18px] shrink-0 accent-[var(--k-accent)]"
        />
        <span className="text-[13px] leading-relaxed text-ink">
          I allow Knowlith to read this folder.
        </span>
      </label>
    </div>
  )
}
