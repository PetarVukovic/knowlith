import { useState, type ReactNode } from "react"
import { Navigate, useNavigate, useParams } from "react-router-dom"
import { ChevronDown, Clock, Pencil } from "lucide-react"
import {
  ImpactStrip,
  KindIcon,
  MonoId,
  RelationList,
  SubtypeLabel,
  TrustStrip,
  kindMeta,
} from "@/components/Domain"
import { EvidenceList } from "@/components/Evidence"
import { Markdown } from "@/components/Markdown"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "@/components/ui/dialog"
import { Textarea } from "@/components/ui/input"
import { TryInAi } from "@/components/TryInAi"
import { api } from "@/lib/api"
import type { ContextObject } from "@/lib/types"
import { formatDate, formatRelative } from "@/lib/utils"
import { useApp } from "@/state/AppState"

/**
 * One claim and the quote it rests on — same composition, two columns.
 *
 * Dependencies / history stay under More. A third app column and a permanent
 * object tree are not how an SMB owner decides if a sentence is true.
 */
export function Workspace() {
  const { objectId } = useParams()
  const { objects, mode, ready, refresh, companyName } = useApp()
  const navigate = useNavigate()
  const [moreOpen, setMoreOpen] = useState(false)
  const [suggestOpen, setSuggestOpen] = useState(false)
  const [draft, setDraft] = useState("")
  const [saving, setSaving] = useState(false)
  const [note, setNote] = useState<string | null>(null)

  const object = objects.find((o) => o.id === objectId)
  if (!object) return ready ? <Navigate to="/browse" replace /> : null

  const openSuggest = () => {
    setDraft(object.body)
    setNote(null)
    setSuggestOpen(true)
  }

  const submitSuggest = async () => {
    const body = draft.trim()
    if (!body || body === object.body) {
      setSuggestOpen(false)
      return
    }
    setSaving(true)
    const result = await api.suggestChange(object.id, body)
    setSaving(false)
    if (!result) {
      setNote("That could not be saved. Try again when Knowlith is running.")
      return
    }
    setSuggestOpen(false)
    await refresh()
    navigate("/review")
  }

  return (
    <div className="mx-auto w-full max-w-[960px] px-4 py-7 sm:px-8">
      <div className="flex flex-wrap items-center gap-2 text-[12px]">
        <KindIcon kind={object.kind} />
        <span className="text-muted">{kindMeta(object.kind).label}</span>
        <SubtypeLabel subtype={object.subtype} />
        {mode === "engineer" ? <span className="font-mono text-faint">{object.path}</span> : null}
        {mode === "engineer" ? <MonoId id={object.id} /> : null}
      </div>
      <p className="mt-1.5 max-w-[54ch] text-[12.5px] text-faint">{kindMeta(object.kind).meaning}</p>

      <h1 className="mt-3 text-[20px] font-semibold tracking-[-0.015em] text-ink">{object.title}</h1>

      <TryInAi
        id={object.id}
        title={object.title}
        kind={object.kind}
        status={object.status}
        company={companyName}
      />

      <div className="mt-6 grid gap-8 lg:grid-cols-2 lg:gap-10">
        <section className="min-w-0">
          <div className="prose-claim">
            <Markdown source={object.body} />
          </div>

          <div className="mt-6 grid gap-3 border-t border-line pt-4">
            <TrustStrip
              status={object.status}
              confidence={object.confidence}
              evidenceCount={object.evidence.length}
              decidedBy={object.decidedBy}
              editedOnApproval={object.editedOnApproval}
              compact
            />
            <ImpactStrip
              relations={object.relations}
              objectId={object.id}
              onOpen={() => setMoreOpen(true)}
              onReads={() => navigate("/activity")}
            />
            <div>
              <Button variant="default" size="sm" onClick={openSuggest}>
                <Pencil />
                Suggest a change
              </Button>
            </div>
          </div>
        </section>

        <aside className="min-w-0">
          <h2 className="text-[13px] font-semibold text-ink">Where this comes from</h2>
          <p className="mt-1 text-[12.5px] text-muted">
            Open a quote to see it in the original document.
          </p>
          <div className="mt-3">
            <EvidenceList items={object.evidence} />
          </div>
        </aside>
      </div>

      <MoreAbout object={object} open={moreOpen} onToggle={() => setMoreOpen((o) => !o)} />

      <Dialog open={suggestOpen} onOpenChange={setSuggestOpen}>
        <DialogContent className="max-w-[520px]">
          <DialogTitle>Suggest a change</DialogTitle>
          <DialogDescription>
            Edit the wording. It goes to your Inbox and is not live for AI tools until you approve it
            again. The source quote stays the same.
          </DialogDescription>
          <Textarea
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            rows={8}
            className="mt-3 font-sans text-[13.5px]"
          />
          {note ? <p className="mt-2 text-[12.5px] text-conflict">{note}</p> : null}
          <div className="mt-4 flex justify-end gap-2">
            <DialogClose asChild>
              <Button variant="ghost" size="sm">
                Cancel
              </Button>
            </DialogClose>
            <Button variant="primary" size="sm" disabled={saving} onClick={() => void submitSuggest()}>
              {saving ? "Saving…" : "Send to Inbox"}
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  )
}

function MoreAbout({
  object,
  open,
  onToggle,
}: {
  object: ContextObject
  open: boolean
  onToggle: () => void
}) {
  const { mode } = useApp()
  const navigate = useNavigate()
  const dependsOn = object.relations.filter((r) => r.type === "depends_on" || r.type === "derived_from")
  const usedBy = object.relations.filter((r) => r.type === "used_by")
  const openId = (id: string) => navigate(`/workspace/${encodeURIComponent(id)}`)

  return (
    <div className="mt-10 border-t border-line pt-4">
      <button
        type="button"
        onClick={onToggle}
        className="flex items-center gap-1.5 text-[13px] font-medium text-muted hover:text-ink"
      >
        <ChevronDown className={`size-3.5 transition-transform ${open ? "rotate-180" : ""}`} />
        More about this
      </button>

      {open ? (
        <div className="mt-4 grid gap-6 sm:grid-cols-2">
          <div>
            <div className="mb-1.5 label-xs">If you change this</div>
            <RelationList
              relations={usedBy}
              onOpen={openId}
              emptyLabel="Nothing else rests on this yet."
            />
            <div className="mb-1.5 mt-4 label-xs">This uses</div>
            <RelationList
              relations={dependsOn}
              onOpen={openId}
              emptyLabel="Nothing. It stands on its own."
            />
          </div>
          <div>
            <div className="mb-1.5 label-xs">History</div>
            <div className="grid gap-2 text-[12.5px]">
              <Row label="Version" value={`v${object.version}`} />
              <Row label="In effect from" value={formatDate(object.validFrom)} />
              <Row label="In effect until" value={object.validTo ? formatDate(object.validTo) : "still current"} />
              <Row label="Last change" value={formatRelative(object.updatedAt)} />
              {object.supersedes ? (
                <Row
                  label="Replaced"
                  value={
                    <button
                      type="button"
                      className="text-accent underline-offset-4 hover:underline"
                      onClick={() => openId(object.supersedes!)}
                    >
                      {object.supersedesTitle ?? "an earlier version"}
                    </button>
                  }
                />
              ) : null}
              {object.supersededBy ? (
                <Row
                  label="Replaced by"
                  value={
                    <button
                      type="button"
                      className="text-accent underline-offset-4 hover:underline"
                      onClick={() => openId(object.supersededBy!.targetId)}
                    >
                      {object.supersededBy.targetTitle}
                    </button>
                  }
                />
              ) : null}
            </div>
            <div className="mt-3 flex items-start gap-2 rounded-md border border-line bg-surface-2 p-2.5 text-[12px] text-muted">
              <Clock className="mt-0.5 size-3.5 shrink-0 text-faint" />
              Older versions are kept. An agent asking what applied last March gets the March answer.
            </div>
            {mode === "engineer" ? (
              <pre className="scroll-thin mt-3 overflow-x-auto rounded-md border border-line bg-surface p-3 font-mono text-[11.5px] leading-relaxed text-muted">
                {JSON.stringify(
                  {
                    id: object.id,
                    kind: object.kind,
                    status: object.status,
                    confidence: object.confidence,
                    path: object.path,
                    evidence: object.evidence.map((e) => ({
                      document: e.documentName,
                      start_byte: e.startByte,
                      end_byte: e.endByte,
                      verified: e.verified,
                    })),
                  },
                  null,
                  2,
                )}
              </pre>
            ) : null}
          </div>
        </div>
      ) : null}
    </div>
  )
}

function Row({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="flex items-baseline justify-between gap-3">
      <span className="shrink-0 text-faint">{label}</span>
      <span className="truncate text-ink">{value}</span>
    </div>
  )
}
