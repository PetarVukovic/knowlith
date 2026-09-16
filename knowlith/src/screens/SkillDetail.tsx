import { Navigate, useNavigate, useParams } from "react-router-dom"
import { ArrowDownToLine, ArrowUpFromLine } from "lucide-react"
import { ImpactStrip, MonoId, RelationList, TrustStrip, kindMeta } from "@/components/Domain"
import { EvidenceList } from "@/components/Evidence"
import { Markdown } from "@/components/Markdown"
import { TryInAi } from "@/components/TryInAi"
import { Panel, PanelHeader } from "@/components/ui/surface"
import { formatRelative } from "@/lib/utils"
import { useApp } from "@/state/AppState"

export function SkillDetail() {
  const { skillId } = useParams()
  const { skills, mode, companyName } = useApp()
  const navigate = useNavigate()

  const skill = skills.find((s) => s.id === skillId)
  if (!skill) return <Navigate to="/browse" replace />

  const open = (id: string) => navigate(`/workspace/${encodeURIComponent(id)}`)

  return (
    <div className="mx-auto w-full max-w-[880px] px-4 py-8">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="text-[12px] text-muted">{kindMeta("skill").label}</p>
          <p className="mt-1 max-w-[54ch] text-[12.5px] text-faint">{kindMeta("skill").meaning}</p>
          <h1 className="mt-2 text-[20px] font-semibold tracking-[-0.015em] text-ink">{skill.name}</h1>
          <p className="mt-1.5 max-w-[62ch] text-[13px] text-muted">{skill.description}</p>
          <p className="mt-2 max-w-[62ch] text-[12.5px] text-muted">
            {skill.draftedFrom ? (
              <>
                Drafted by Knowlith when you confirmed the process{" "}
                <button
                  type="button"
                  className="text-accent underline-offset-4 hover:underline"
                  onClick={() => open(skill.draftedFrom!.targetId)}
                >
                  {skill.draftedFrom.targetTitle}
                </button>
                . It rests on {skill.requires.length}{" "}
                {skill.requires.length === 1 ? "confirmed item" : "confirmed items"} listed under Built
                on, and on nothing else.
              </>
            ) : (
              "Not drafted from a process on record."
            )}
          </p>
          <TrustStrip
            className="mt-3"
            status={skill.status}
            confidence={skill.confidence}
            evidenceCount={skill.evidence.length}
            decidedBy={skill.decidedBy ?? undefined}
          />
          <ImpactStrip className="mt-2" relations={skill.affects} objectId={skill.id} />
          <div className="mt-2 flex flex-wrap items-center gap-3 text-[12px] text-faint">
            <span>v{skill.version}</span>
            <span>updated {formatRelative(skill.updatedAt)}</span>
            {mode === "engineer" ? <MonoId id={skill.id} /> : null}
          </div>
        </div>
      </div>

      <TryInAi
        id={skill.id}
        title={skill.name}
        kind="skill"
        status={skill.status}
        company={companyName}
      />

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
              <PanelHeader
                title={
                  <span className="flex items-center gap-1.5">
                    <ArrowDownToLine className="size-3.5 text-faint" />
                    It needs
                  </span>
                }
              />
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
              <PanelHeader
                title={
                  <span className="flex items-center gap-1.5">
                    <ArrowUpFromLine className="size-3.5 text-faint" />
                    It produces
                  </span>
                }
              />
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

        <aside className="grid gap-5 self-start">
          <Panel>
            <PanelHeader title="Built on" />
            <div className="p-4">
              <RelationList relations={skill.requires} onOpen={open} emptyLabel="Nothing listed yet." />
            </div>
          </Panel>
          <Panel>
            <PanelHeader title="Used in" />
            <div className="p-4">
              <RelationList
                relations={skill.affects}
                onOpen={open}
                emptyLabel="Not used by a process yet."
              />
            </div>
          </Panel>
          <Panel>
            <PanelHeader title="Where it comes from" />
            <div className="p-4">
              <EvidenceList items={skill.evidence} />
            </div>
          </Panel>
        </aside>
      </div>
    </div>
  )
}
