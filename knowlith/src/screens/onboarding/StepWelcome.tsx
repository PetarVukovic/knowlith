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
}: {
  name: string
  onName: (v: string) => void
  logo: string | null
  onLogo: (v: string | null) => void
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
        Knowlith reads the documents your team already works from and turns them into one answer every AI tool
        gives. You approve everything before it counts.
      </p>

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
