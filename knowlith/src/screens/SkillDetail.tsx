import { Navigate, useNavigate, useParams } from "react-router-dom"
import { ArrowDownToLine, ArrowUpFromLine, Play } from "lucide-react"
import { ImpactStrip, MonoId, RelationList, TrustStrip } from "@/components/Domain"
import { EvidenceList } from "@/components/Evidence"
import { Markdown } from "@/components/Markdown"
import { Button } from "@/components/ui/button"
import { Panel, PanelHeader } from "@/components/ui/surface"
import { formatRelative } from "@/lib/utils"
import { useApp } from "@/state/AppState"

export function SkillDetail() {
  const { skillId } = useParams()
  const { skills, mode } = useApp()
  const navigate = useNavigate()

  const skill = skills.find((s) => s.id === skillId)
  if (!skill) return <Navigate to="/workspace" replace />

  const open = (id: string) => navigate(`/workspace/${encodeURIComponent(id)}`)

  return (
    <div className="mx-auto w-full max-w-[880px] px-4 py-8">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <h1 className="text-[20px] font-semibold tracking-[-0.015em] text-ink">{skill.name}</h1>
          <p className="mt-1.5 max-w-[62ch] text-[13px] text-muted">{skill.description}</p>
          <TrustStrip
            className="mt-3"
            status={skill.status}
            confidence={skill.evidence.length > 2 ? 0.95 : 0.88}
            evidenceCount={skill.evidence.length}
            decidedBy={skill.status === "approved" ? "Ana Kovač" : undefined}
          />
          <ImpactStrip className="mt-2" relations={skill.affects} objectId={skill.id} />
          <div className="mt-2 flex flex-wrap items-center gap-3 text-[12px] text-faint">
            <span>v{skill.version}</span>
            <span>updated {formatRelative(skill.updatedAt)}</span>
            {mode === "engineer" ? <MonoId id={skill.id} /> : null}
          </div>
        </div>
        <Button variant="primary">
          <Play />
          Try this skill
        </Button>
      </div>

      <div className="mt-7 grid gap-5 lg:grid-cols-[1fr_300px]">
        <div className="min-w-0 grid gap-5">
          <Panel>
            <PanelHeader
              title="What it tells the AI to do"
              description="This is the whole instruction. Nothing is hidden behind it."
            />
            <div className="p-4">
              <Markdown source={skill.markdown} />
            </div>
          </Panel>

          <div className="grid gap-3 sm:grid-cols-2">
            <Panel>
              <PanelHeader title={<span className="flex items-center gap-1.5"><ArrowDownToLine className="size-3.5 text-faint" />It needs</span>} />
              <ul className="grid gap-2.5 p-4">
                {skill.inputs.map((io) => (
                  <li key={io.name}>
                    <div className="flex items-baseline gap-2">
                      <span className="font-mono text-[12px] text-ink">{io.name}</span>
                      <span className="text-[11px] text-faint">{io.type}</span>
                    </div>
                    <div className="text-[12px] text-muted">{io.description}</div>
                  </li>
                ))}
              </ul>
            </Panel>
            <Panel>
              <PanelHeader title={<span className="flex items-center gap-1.5"><ArrowUpFromLine className="size-3.5 text-faint" />It produces</span>} />
              <ul className="grid gap-2.5 p-4">
                {skill.outputs.map((io) => (
                  <li key={io.name}>
                    <div className="flex items-baseline gap-2">
                      <span className="font-mono text-[12px] text-ink">{io.name}</span>
                      <span className="text-[11px] text-faint">{io.type}</span>
                    </div>
                    <div className="text-[12px] text-muted">{io.description}</div>
                  </li>
                ))}
              </ul>
            </Panel>
          </div>
        </div>

        <aside className="grid content-start gap-5">
          <Panel>
            <PanelHeader title="Built on" description="If one of these changes, this skill changes with it." />
            <div className="p-2">
              <RelationList relations={skill.requires} onOpen={open} />
            </div>
          </Panel>

          <Panel>
            <PanelHeader title="Used in" />
            <div className="p-2">
              <RelationList relations={skill.affects} onOpen={open} emptyLabel="Not used by a process yet." />
            </div>
          </Panel>

          <div>
            <div className="mb-2 flex items-baseline justify-between">
              <h2 className="text-[13px] font-semibold text-ink">Where it comes from</h2>
              <span className="text-[12px] text-faint">{skill.evidence.length}</span>
            </div>
            <EvidenceList items={skill.evidence} compact />
            <p className="mt-2 text-[12px] text-muted">
              A skill is only as trustworthy as the rules under it. Open any quote to see the original document.
            </p>
          </div>
        </aside>
      </div>
    </div>
  )
}
