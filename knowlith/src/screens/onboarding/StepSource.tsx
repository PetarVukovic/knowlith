import { useEffect, useRef, useState } from "react"
import { Check, CloudCog, FolderOpen, HardDrive, Loader2, Server } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { demoInventory, inventoryOf, type Inventory } from "@/lib/inventory"
import type { SourceKind } from "@/lib/types"
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

export function StepSource({
  kind,
  onKind,
  onPick,
  inventory,
}: {
  kind: SourceKind | null
  onKind: (kind: SourceKind) => void
  onPick: (kind: SourceKind, path: string, inventory: Inventory) => void
  inventory: Inventory | null
}) {
  const picker = useRef<HTMLInputElement>(null)
  const [nasPath, setNasPath] = useState("")
  const [reading, setReading] = useState(false)

  // `webkitdirectory` is not in the React attribute types, and setting it on
  // the element is the only way to get a real folder from the picker.
  useEffect(() => {
    const el = picker.current
    if (!el) return
    el.setAttribute("webkitdirectory", "")
    el.setAttribute("directory", "")
  }, [])

  const readFolder = (list: FileList | null) => {
    if (!list || list.length === 0) return
    setReading(true)
    const files = Array.from(list)
    const counted = inventoryOf(files)
    onPick("folder", counted.folderName, counted)
    setReading(false)
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
              disabled={!option.available}
              onClick={() => {
                if (option.kind === "folder") {
                  onKind("folder")
                  picker.current?.click()
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

      <input
        ref={picker}
        type="file"
        multiple
        className="hidden"
        onChange={(e) => readFolder(e.target.files)}
      />

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
              placeholder="\\\\termoval-nas\\Zajednicko"
              className="font-mono text-[12.5px]"
            />
            <Button
              className="shrink-0"
              disabled={nasPath.trim().length < 3 || reading}
              onClick={() => {
                // A browser cannot walk a network share; the count comes back
                // from the machine Knowlith runs on.
                setReading(true)
                window.setTimeout(() => {
                  onPick("nas", nasPath.trim(), { ...demoInventory, folderName: nasPath.trim() })
                  setReading(false)
                }, 900)
              }}
            >
              Look inside
            </Button>
          </div>
        </div>
      ) : null}

      {reading ? (
        <p className="mt-6 flex items-center gap-2 text-[13px] text-muted">
          <Loader2 className="size-4 animate-spin text-accent" />
          Counting the folder…
        </p>
      ) : null}

      {inventory ? (
        <div className="mt-6 flex flex-wrap items-center gap-x-5 gap-y-2 rounded-lg border border-line bg-surface-2 px-4 py-3">
          <HardDrive className="size-4 shrink-0 text-faint" />
          <span className="text-[13px] font-medium text-ink">{inventory.folderName}</span>
          <span className="tabular text-[12.5px] text-muted">
            {formatCount(inventory.fileCount)} files · {formatBytes(inventory.bytes)}
          </span>
          <Button variant="ghost" size="sm" className="ml-auto" onClick={() => picker.current?.click()}>
            Choose another
          </Button>
        </div>
      ) : null}

      {!inventory ? (
        <button
          type="button"
          onClick={() => onPick("folder", demoInventory.folderName, demoInventory)}
          className="mt-6 text-[12px] text-faint underline-offset-4 transition-colors hover:text-muted hover:underline"
        >
          I don't have files handy — use the example company
        </button>
      ) : null}
    </div>
  )
}
