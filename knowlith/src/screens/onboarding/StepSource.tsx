import { useState } from "react"
import { Check, CloudCog, FolderOpen, HardDrive, Loader2, Server } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { failed, folders } from "@/lib/api"
import type { Inventory, SourceKind } from "@/lib/types"
import { cn, formatBytes, formatCount } from "@/lib/utils"

/**
 * Where the company's knowledge already lives.
 *
 * Only the two that work are offered as real choices. A connector that opens a
 * waiting list is still worth showing — it tells the owner the product knows
 * their storage exists — but it never looks selectable.
 */
const OPTIONS: {
  kind: SourceKind | "gdrive" | "onedrive"
  label: string
  detail: string
  Icon: typeof FolderOpen
  available: boolean
}[] = [
  {
    kind: "folder",
    label: "Folder on this Mac",
    detail: "Sales, offers, contracts — wherever the files already are.",
    Icon: FolderOpen,
    available: true,
  },
  {
    kind: "nas",
    label: "Network drive",
    detail: "A shared folder on the office NAS or server.",
    Icon: Server,
    available: true,
  },
  { kind: "gdrive", label: "Google Drive", detail: "Coming soon.", Icon: CloudCog, available: false },
  { kind: "onedrive", label: "OneDrive", detail: "Coming soon.", Icon: CloudCog, available: false },
]

/**
 * The folder itself comes from the machine, not from the browser.
 *
 * A page is never told where a folder is, so "Folder on this Mac" asks the
 * daemon to open the system chooser and answer with a real path. The network
 * drive is typed, because a chooser will not show a share that is not
 * mounted, and then checked against the same walk — so both routes arrive at
 * the same counted folder.
 */
export function StepSource({
  kind,
  onKind,
  onPick,
  inventory,
  path,
}: {
  kind: SourceKind | null
  onKind: (kind: SourceKind) => void
  onPick: (kind: SourceKind, path: string, inventory: Inventory) => void
  inventory: Inventory | null
  path: string
}) {
  const [nasPath, setNasPath] = useState("")
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const choose = async () => {
    onKind("folder")
    setBusy(true)
    setError(null)
    const result = await folders.browse()
    setBusy(false)
    if (failed(result)) return setError(result.error)
    // Closing the chooser without picking leaves the screen where it was.
    if (!result.chosen || !result.inventory) return
    onPick("folder", result.chosen, result.inventory)
  }

  const look = async (where: string) => {
    setBusy(true)
    setError(null)
    const result = await folders.preview(where)
    setBusy(false)
    if (failed(result)) return setError(result.error)
    if (!result.chosen || !result.inventory) return
    onPick("nas", result.chosen, result.inventory)
  }

  return (
    <div>
      <h1 className="text-[26px] font-semibold leading-tight tracking-[-0.022em] text-ink">Add your first source</h1>
      <p className="mt-2.5 max-w-[48ch] text-[14px] leading-relaxed text-muted">
        Point Knowlith at one folder your team already works from. You can add more later.
      </p>

      <div className="mt-8 grid gap-2.5 sm:grid-cols-2">
        {OPTIONS.map((option) => {
          const selected = option.available && kind === option.kind
          return (
            <button
              key={option.kind}
              type="button"
              disabled={!option.available || busy}
              onClick={() => {
                if (option.kind === "folder") {
                  void choose()
                } else if (option.kind === "nas") {
                  onKind("nas")
                }
              }}
              className={cn(
                "group flex flex-col items-start gap-2 rounded-xl border p-4 text-left transition-all",
                option.available
                  ? "border-line bg-surface hover:border-accent/50 hover:bg-accent-soft/40"
                  : "cursor-not-allowed border-dashed border-line bg-surface-2 opacity-60",
                selected && "border-accent bg-accent-soft",
              )}
            >
              <span className="flex w-full items-center gap-2">
                <option.Icon
                  className={cn("size-[18px] shrink-0", selected ? "text-accent" : "text-faint")}
                />
                <span className="text-[14px] font-medium text-ink">{option.label}</span>
                {selected ? <Check className="ml-auto size-4 text-accent" /> : null}
              </span>
              <span className="text-[12.5px] leading-relaxed text-muted">{option.detail}</span>
            </button>
          )
        })}
      </div>

      {kind === "nas" ? (
        <div className="mt-5">
          <label htmlFor="nas-path" className="mb-1.5 block text-[12.5px] font-medium text-ink">
            Network location
          </label>
          <div className="flex gap-2">
            <Input
              id="nas-path"
              value={nasPath}
              onChange={(e) => setNasPath(e.target.value)}
              placeholder="/Volumes/Zajednicko"
              spellCheck={false}
              className="font-mono text-[12.5px]"
              onKeyDown={(e) => e.key === "Enter" && nasPath.trim() && void look(nasPath.trim())}
            />
            <Button
              className="shrink-0"
              disabled={nasPath.trim().length < 3 || busy}
              onClick={() => void look(nasPath.trim())}
            >
              Look inside
            </Button>
          </div>
          <p className="mt-1.5 text-[12px] text-faint">
            The share has to be mounted on this Mac first — Knowlith reads it as a folder.
          </p>
        </div>
      ) : null}

      {busy ? (
        <p className="mt-6 flex items-center gap-2 text-[13px] text-muted">
          <Loader2 className="size-4 animate-spin text-accent" />
          {kind === "nas" ? "Counting the folder…" : "Waiting for the chooser…"}
        </p>
      ) : null}

      {error ? <p className="mt-6 text-[13px] text-conflict">{error}</p> : null}

      {inventory && !busy ? (
        <div className="mt-6 flex flex-wrap items-center gap-x-5 gap-y-2 rounded-lg border border-line bg-surface-2 px-4 py-3">
          <HardDrive className="size-4 shrink-0 text-faint" />
          <span className="min-w-0 flex-1 truncate text-[13px] font-medium text-ink">{path}</span>
          <span className="tabular text-[12.5px] text-muted">
            {formatCount(inventory.readable)} readable · {formatBytes(inventory.bytes)}
          </span>
          <Button variant="ghost" size="sm" onClick={() => void choose()}>
            Choose another
          </Button>
        </div>
      ) : null}

      {inventory && inventory.readable === 0 && !busy ? (
        <p className="mt-3 text-[13px] text-pending">
          Nothing in this folder can be read. Knowlith reads documents and spreadsheets.
        </p>
      ) : null}
    </div>
  )
}
