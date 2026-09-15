import { useState } from "react"
import { Navigate, useNavigate, useParams } from "react-router-dom"
import { Clock, PanelRightClose, PanelRightOpen, Pencil } from "lucide-react"
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
import { ResizeHandle, usePanelSize } from "@/components/Resizable"
import { Markdown } from "@/components/Markdown"
import { Button } from "@/components/ui/button"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import type { ContextObject } from "@/lib/types"
import { formatDate, formatRelative } from "@/lib/utils"
import { useApp } from "@/state/AppState"

type InspectorTab = "evidence" | "dependencies" | "history" | "raw"

export function Workspace() {
  const { objectId } = useParams()
  const { objects, mode, ready } = useApp()
  const [inspectorOpen, setInspectorOpen] = useState(true)
  const inspector = usePanelSize("inspector", 380, 280, 720)
  const [tab, setTab] = useState<InspectorTab>("evidence")

  const object = objects.find((o) => o.id === objectId)
  if (!object) return ready ? <Navigate to="/home" replace /> : null

  return (
    <div className="flex min-h-full">
      <article className="min-w-0 flex-1 px-4 py-7 sm:px-8">
        <div className="mx-auto max-w-[680px]">
          <div className="flex flex-wrap items-center gap-2 text-[12px]">
            <KindIcon kind={object.kind} />
            <span className="text-[12px] text-muted">{kindMeta(object.kind).label}</span>
            <SubtypeLabel subtype={object.subtype} />
            {/* The file path is an implementation detail of the Lake, not
                something an owner should have to parse. */}
            {mode === "engineer" ? <span className="font-mono text-faint">{object.path}</span> : null}
            <span className="ml-auto flex items-center gap-2">
              <Button
                variant="ghost"
                size="icon-sm"
                onClick={() => setInspectorOpen((o) => !o)}
                aria-label={inspectorOpen ? "Hide details" : "Show details"}
                className="lg:hidden"
              >
                {inspectorOpen ? <PanelRightClose /> : <PanelRightOpen />}
              </Button>
            </span>
          </div>

          <div className="mt-4">
            <Markdown source={object.body} />
          </div>

          <div className="mt-8 grid gap-3 border-t border-line pt-4">
            <TrustStrip
              status={object.status}
              confidence={object.confidence}
              evidenceCount={object.evidence.length}
              decidedBy={object.decidedBy}
              editedOnApproval={object.editedOnApproval}
            />
            <ImpactStrip
              relations={object.relations}
              objectId={object.id}
              onOpen={() => {
                setTab("dependencies")
                setInspectorOpen(true)
              }}
            />
            <div>
              <Button variant="default" size="sm">
                <Pencil />
                Suggest a change
              </Button>
            </div>
          </div>

          <div className="mt-8 lg:hidden">
            {inspectorOpen ? <Inspector object={object} tab={tab} onTab={setTab} /> : null}
          </div>
        </div>
      </article>

      <ResizeHandle panel={inspector} edge="end" label="Resize details panel" className="hidden lg:block" />
      <aside
        className="scroll-thin hidden shrink-0 overflow-y-auto bg-bg px-4 py-7 lg:block"
        style={{ width: inspector.width }}
      >
        <Inspector object={object} tab={tab} onTab={setTab} />
      </aside>
    </div>
  )
}

function Inspector({
  object,
  tab,
  onTab,
}: {
  object: ContextObject
  tab: InspectorTab
  onTab: (t: InspectorTab) => void
}) {
  const { mode } = useApp()
  const navigate = useNavigate()

  const dependsOn = object.relations.filter((r) => r.type === "depends_on" || r.type === "derived_from")
  const usedBy = object.relations.filter((r) => r.type === "used_by")
  const open = (id: string) => navigate(`/workspace/${encodeURIComponent(id)}`)

  return (
    <Tabs value={tab} onValueChange={(v) => onTab(v as InspectorTab)} className="grid gap-4">
      <TabsList>
        <TabsTrigger value="evidence">Evidence</TabsTrigger>
        <TabsTrigger value="dependencies">
          Dependencies
          {usedBy.length > 0 ? <span className="ml-1 text-faint">{usedBy.length}</span> : null}
        </TabsTrigger>
        <TabsTrigger value="history">History</TabsTrigger>
        {mode === "engineer" ? <TabsTrigger value="raw">Raw</TabsTrigger> : null}
      </TabsList>

      <TabsContent value="evidence" className="grid gap-2">
        <p className="text-[12px] text-muted">
          Every sentence above comes from one of these. Open one to see it in the original document.
        </p>
        <EvidenceList items={object.evidence} />
      </TabsContent>

      <TabsContent value="dependencies" className="grid gap-4">
        <div>
          <div className="mb-1.5 label-xs">This uses</div>
          <RelationList relations={dependsOn} onOpen={open} emptyLabel="Nothing. It stands on its own." />
        </div>
        <div>
          <div className="mb-1.5 label-xs">Change this and these change</div>
          <RelationList relations={usedBy} onOpen={open} />
        </div>
        <ImpactStrip relations={object.relations} objectId={object.id} />
      </TabsContent>

      <TabsContent value="history" className="grid gap-3">
        <div className="grid gap-2 text-[12.5px]">
          <Row label="Version" value={`v${object.version}`} />
          <Row label="In effect from" value={formatDate(object.validFrom)} />
          <Row label="In effect until" value={object.validTo ? formatDate(object.validTo) : "still current"} />
          <Row label="Last change" value={formatRelative(object.updatedAt)} />
          {object.supersedes && mode === "engineer" ? <Row label="Replaced" value={object.supersedes} mono /> : null}
        </div>
        <div className="flex items-start gap-2 rounded-md border border-line bg-surface-2 p-2.5 text-[12px] text-muted">
          <Clock className="mt-0.5 size-3.5 shrink-0 text-faint" />
          Older versions are kept, not deleted. An agent asking "what applied in March" gets the March answer.
        </div>
      </TabsContent>

      {mode === "engineer" ? (
        <TabsContent value="raw" className="grid gap-2">
          <MonoId id={object.id} />
          <pre className="scroll-thin overflow-x-auto rounded-md border border-line bg-surface p-3 font-mono text-[11.5px] leading-relaxed text-muted">
            {JSON.stringify(
              {
                id: object.id,
                kind: object.kind,
                subtype: object.subtype,
                status: object.status,
                version: object.version,
                confidence: object.confidence,
                path: object.path,
                valid_from: object.validFrom,
                valid_to: object.validTo,
                supersedes: object.supersedes,
                evidence: object.evidence.map((e) => ({
                  document: e.documentName,
                  start_byte: e.startByte,
                  end_byte: e.endByte,
                  verified: e.verified,
                })),
                relations: object.relations,
              },
              null,
              2,
            )}
          </pre>
        </TabsContent>
      ) : null}
    </Tabs>
  )
}

function Row({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="flex items-baseline justify-between gap-3">
      <span className="shrink-0 text-faint">{label}</span>
      <span className={mono ? "truncate font-mono text-[11.5px] text-ink" : "truncate text-ink"}>{value}</span>
    </div>
  )
}
