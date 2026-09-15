import { CopyMinus, FileQuestion, History } from "lucide-react"
import { Button } from "@/components/ui/button"
import type { Inventory } from "@/lib/inventory"
import { formatBytes, formatCount, formatRelative } from "@/lib/utils"

/**
 * What is in the folder, counted without reading anything.
 *
 * Names, sizes and dates are enough for this screen, and stopping there is the
 * point: the owner sees a true picture of their own folder before a single
 * document has been opened, which is the last moment they can still say no.
 */
export function StepPreview({
  inventory,
  path,
  onBuild,
}: {
  inventory: Inventory
  path: string
  onBuild: () => void
}) {
  const notable = [
    inventory.duplicates > 0
      ? { Icon: CopyMinus, label: `${formatCount(inventory.duplicates)} duplicates`, detail: "Read once, not twice." }
      : null,
    inventory.oldVersions > 0
      ? {
          Icon: History,
          label: `${formatCount(inventory.oldVersions)} look like old versions`,
          detail: "Kept, but the newer file wins where they disagree.",
        }
      : null,
    inventory.unsupported > 0
      ? {
          Icon: FileQuestion,
          label: `${formatCount(inventory.unsupported)} file types not read`,
          detail: "Images, drawings, archives and the like.",
        }
      : null,
  ].filter((x) => x !== null)

  const readable = inventory.fileTypes.reduce((sum, t) => sum + t.count, 0)

  return (
    <div>
      <h1 className="text-[26px] font-semibold leading-tight tracking-[-0.022em] text-ink">
        Here is what's in {path || inventory.folderName}
      </h1>
      <p className="mt-2.5 text-[14px] leading-relaxed text-muted">
        Counted from the file list only. Nothing has been opened yet.
      </p>

      <div className="mt-8 grid grid-cols-2 gap-px overflow-hidden rounded-xl border border-line bg-line sm:grid-cols-3">
        <Tile value={formatCount(inventory.fileCount)} label="files" />
        <Tile value={formatBytes(inventory.bytes)} label="on disk" />
        <Tile value={formatCount(readable)} label="Knowlith can read" />
      </div>

      {inventory.fileTypes.length > 0 ? (
        <ul className="mt-5 grid gap-2">
          {inventory.fileTypes.map((type) => {
            const share = readable > 0 ? Math.round((type.count / readable) * 100) : 0
            return (
              <li key={type.ext} className="flex items-center gap-3">
                <span className="w-12 shrink-0 text-[12.5px] font-medium text-ink">{type.ext}</span>
                <span className="h-1.5 flex-1 overflow-hidden rounded-full bg-surface-3">
                  <span className="block h-full rounded-full bg-accent/60" style={{ width: `${share}%` }} />
                </span>
                <span className="tabular w-14 shrink-0 text-right text-[12.5px] text-muted">
                  {formatCount(type.count)}
                </span>
              </li>
            )
          })}
        </ul>
      ) : null}

      {notable.length > 0 ? (
        <ul className="mt-7 grid gap-3 border-t border-line pt-5">
          {notable.map((item) => (
            <li key={item.label} className="flex gap-2.5">
              <item.Icon className="mt-0.5 size-4 shrink-0 text-faint" />
              <span className="text-[13px] leading-relaxed">
                <span className="font-medium text-ink">{item.label}</span>
                <span className="text-muted"> — {item.detail}</span>
              </span>
            </li>
          ))}
        </ul>
      ) : null}

      {inventory.newest ? (
        <p className="mt-5 text-[12px] text-faint">
          Most recent file changed {formatRelative(inventory.newest)}.
        </p>
      ) : null}

      <div className="mt-9">
        <Button size="lg" variant="primary" onClick={onBuild}>
          Build company context
        </Button>
        <p className="mt-2.5 text-[12px] text-faint">
          You can stop this at any time. Nothing becomes usable until you approve it.
        </p>
      </div>
    </div>
  )
}

function Tile({ value, label }: { value: string; label: string }) {
  return (
    <div className="bg-surface px-4 py-4">
      <div className="tabular text-[24px] font-semibold leading-none text-ink">{value}</div>
      <div className="mt-1.5 text-[12px] text-muted">{label}</div>
    </div>
  )
}
