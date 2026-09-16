import { useState } from "react"
import { useNavigate } from "react-router-dom"
import { AlertTriangle, FolderOpen, Loader2 } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { failed, folders } from "@/lib/api"
import type { Browsed } from "@/lib/types"
import { formatBytes, formatCount } from "@/lib/utils"
import { useApp } from "@/state/AppState"

/**
 * Pointing Knowlith at a folder.
 *
 * Three states, in the order they happen: choose, look at what was found,
 * agree. The middle one is the reason this is a dialog and not a button —
 * the owner is about to hand over years of their own work, and the honest
 * thing to show before they commit is what was actually found in there,
 * including the part that cannot be read.
 *
 * The chooser is the machine's own. A browser cannot tell a page where a
 * folder is, so the daemon opens the real dialog and answers with a real
 * path; the text field underneath is for a network share the chooser will
 * not show, and for anyone who would rather paste.
 */
export function AddSource({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { addSource } = useApp()
  const navigate = useNavigate()
  const [found, setFound] = useState<Browsed | null>(null)
  const [typed, setTyped] = useState("")
  const [busy, setBusy] = useState<"browsing" | "checking" | "adding" | null>(null)
  const [error, setError] = useState<string | null>(null)

  const reset = () => {
    setFound(null)
    setTyped("")
    setBusy(null)
    setError(null)
  }

  const close = () => {
    reset()
    onClose()
  }

  const browse = async () => {
    setBusy("browsing")
    setError(null)
    const result = await folders.browse()
    setBusy(null)
    if (failed(result)) return setError(result.error)
    // Closing the chooser without picking is an ordinary thing to do, and
    // the dialog goes back to where it was rather than showing an error.
    if (!result.chosen) return
    setFound(result)
    setTyped(result.chosen)
  }

  const check = async () => {
    if (!typed.trim()) return
    setBusy("checking")
    setError(null)
    const result = await folders.preview(typed)
    setBusy(null)
    if (failed(result)) return setError(result.error)
    setFound(result)
  }

  const add = async () => {
    const path = found?.chosen ?? typed
    if (!path.trim()) return
    setBusy("adding")
    setError(null)
    const result = await addSource(path)
    setBusy(null)
    if (typeof result === "string") return setError(result)
    close()
    // Straight to the dashboard, where the work panel is already watching
    // the queue. Leaving the owner on the folder list after adding one is
    // leaving them where nothing visibly happens.
    navigate("/home")
  }

  const inventory = found?.inventory ?? null

  return (
    <Dialog open={open} onOpenChange={(next) => !next && close()}>
      <DialogContent className="max-w-[480px]">
        <DialogTitle>Add a folder</DialogTitle>
        <DialogDescription>
          Knowlith opens the files in here and never writes into them.
        </DialogDescription>

        <div className="mt-5 space-y-4">
          <Button variant="primary" className="w-full" onClick={browse} disabled={busy !== null}>
            {busy === "browsing" ? <Loader2 className="animate-spin" /> : <FolderOpen />}
            {busy === "browsing" ? "Waiting for the chooser…" : "Choose folder…"}
          </Button>

          <div className="flex items-center gap-3 text-[12px] text-muted">
            <div className="h-px flex-1 bg-line" />
            or type a path
            <div className="h-px flex-1 bg-line" />
          </div>

          <div className="flex gap-2">
            <Input
              value={typed}
              placeholder="~/Documents/Sales"
              spellCheck={false}
              onChange={(e) => {
                setTyped(e.target.value)
                // What is on screen describes the old path the moment the
                // text changes, so it goes rather than going stale.
                setFound(null)
                setError(null)
              }}
              onKeyDown={(e) => e.key === "Enter" && void check()}
            />
            <Button variant="ghost" onClick={() => void check()} disabled={busy !== null || !typed.trim()}>
              {busy === "checking" ? <Loader2 className="animate-spin" /> : "Check"}
            </Button>
          </div>

          {error && (
            <p className="flex items-start gap-2 text-[13px] text-conflict">
              <AlertTriangle className="mt-[2px] size-4 shrink-0" />
              {error}
            </p>
          )}

          {inventory && (
            <div className="rounded-lg border border-line bg-raised p-4">
              <p className="truncate text-[13px] font-medium text-ink">{found?.chosen}</p>
              <p className="mt-1 text-[13px] text-muted">
                {formatCount(inventory.readable)} readable {inventory.readable === 1 ? "file" : "files"}
                {" · "}
                {formatBytes(inventory.bytes)}
                {inventory.truncated && " · more than we counted"}
              </p>

              {inventory.types.length > 0 && (
                <p className="mt-2 text-[12px] text-muted">
                  {inventory.types.map((t) => `${t.label} ${t.count}`).join(" · ")}
                </p>
              )}

              {/* Named, not summed: "38 files skipped" tells the owner
                  nothing they can act on; "JPG 38" tells them it was the
                  photos, and that nothing they care about was missed. */}
              {inventory.skipped.length > 0 && (
                <p className="mt-2 text-[12px] text-muted">
                  Cannot be read: {inventory.skipped.map((s) => `${s.label} ${s.count}`).join(" · ")}
                </p>
              )}

              {inventory.readable === 0 && (
                <p className="mt-2 text-[12px] text-pending">
                  Nothing in here can be read. Knowlith reads documents and spreadsheets.
                </p>
              )}
            </div>
          )}

          <div className="flex justify-end gap-2 pt-1">
            <Button variant="ghost" onClick={close}>
              Cancel
            </Button>
            <Button
              variant="primary"
              onClick={() => void add()}
              disabled={busy !== null || !inventory || inventory.readable === 0}
            >
              {busy === "adding" && <Loader2 className="animate-spin" />}
              Add folder
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  )
}
