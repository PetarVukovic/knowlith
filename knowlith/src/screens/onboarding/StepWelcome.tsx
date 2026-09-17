import { useRef } from "react"
import { ImagePlus, X } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { initialsOf } from "@/lib/utils"

/**
 * The first screen says what this is and asks for one thing.
 *
 * Anything else here — industry, team size, use case — is a question the
 * product should be able to answer for itself from the files, and asking it
 * up front makes the tool feel like a form.
 */
export function StepWelcome({
  name,
  onName,
  logo,
  onLogo,
  profile,
  onProfile,
}: {
  name: string
  onName: (v: string) => void
  logo: string | null
  onLogo: (v: string | null) => void
  profile: string
  onProfile: (v: string) => void
}) {
  const fileInput = useRef<HTMLInputElement>(null)
  const initials = initialsOf(name || "Knowlith")

  const readLogo = (file: File | undefined) => {
    if (!file) return
    const reader = new FileReader()
    reader.onload = () => onLogo(typeof reader.result === "string" ? reader.result : null)
    reader.readAsDataURL(file)
  }

  return (
    <div>
      <h1 className="text-[30px] font-semibold leading-[1.15] tracking-[-0.025em] text-ink">
        Turn your company files into shared AI knowledge.
      </h1>
      <p className="mt-3 max-w-[46ch] text-[14px] leading-relaxed text-muted">
        Knowlith reads the documents your team already works from and helps your connected AI tools use your own company knowledge. You approve everything before it counts.
      </p>

      <ol className="mt-6 grid gap-2 rounded-xl border border-line bg-surface-2 p-4 text-[13px]">
        {[
          { n: "1", t: "Add a folder", d: "Contracts, price lists, procedures you already have." },
          { n: "2", t: "Confirm what is true", d: "Rules, processes and terms — with the quote behind each." },
          { n: "3", t: "Connect your AI", d: "Claude, Codex or Cursor Agent answer from what you confirmed." },
        ].map((step) => (
          <li key={step.n} className="flex gap-3">
            <span className="grid size-6 shrink-0 place-items-center rounded-full bg-accent text-[11px] font-semibold text-on-accent">
              {step.n}
            </span>
            <span>
              <span className="font-medium text-ink">{step.t}</span>
              <span className="mt-0.5 block text-[12.5px] text-muted">{step.d}</span>
            </span>
          </li>
        ))}
      </ol>

      <div className="mt-10 grid gap-6">
        <div>
          <label htmlFor="company-name" className="mb-1.5 block text-[12.5px] font-medium text-ink">
            Company name
          </label>
          <Input
            id="company-name"
            value={name}
            onChange={(e) => onName(e.target.value)}
            placeholder="Termoval d.o.o."
            autoFocus
            className="h-11 text-[15px]"
          />
        </div>

        <div>
          <label htmlFor="company-profile" className="mb-1.5 block text-[12.5px] font-medium text-ink">What does your business do? <span className="font-normal text-faint">Optional</span></label>
          <textarea id="company-profile" value={profile} onChange={(e) => onProfile(e.target.value)} maxLength={1500} rows={3} placeholder="For example: We install and service heating systems for homes and small offices." className="w-full resize-y rounded-lg border border-line bg-surface px-3 py-2.5 text-[14px] text-ink outline-none focus:border-accent" />
          <p className="mt-1.5 text-xs text-muted">This helps your AI understand which rules and processes matter to your business.</p>
        </div>
        <div className="flex items-center gap-4">
          <span className="grid size-14 shrink-0 place-items-center overflow-hidden rounded-xl bg-accent text-[17px] font-semibold text-on-accent">
            {logo ? (
              <img src={logo} alt="" className="size-full object-cover" />
            ) : (
              initials
            )}
          </span>
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2">
              <Button variant="default" size="sm" onClick={() => fileInput.current?.click()}>
                <ImagePlus />
                {logo ? "Replace logo" : "Add logo"}
              </Button>
              {logo ? (
                <Button variant="ghost" size="sm" onClick={() => onLogo(null)}>
                  <X />
                  Remove
                </Button>
              ) : null}
            </div>
            <p className="mt-1.5 text-[12px] text-faint">Optional. Without one we use your initials.</p>
          </div>
          <input
            ref={fileInput}
            type="file"
            accept="image/*"
            className="hidden"
            onChange={(e) => readLogo(e.target.files?.[0])}
          />
        </div>
      </div>
    </div>
  )
}
