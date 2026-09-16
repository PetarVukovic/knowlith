import { useEffect, useRef, useState } from "react"
import { Navigate, useNavigate, useParams } from "react-router-dom"
import { ArrowDownToLine, ArrowUpFromLine, Check, Copy, Loader2, Play } from "lucide-react"
import { ImpactStrip, MonoId, RelationList, TrustStrip } from "@/components/Domain"
import { EvidenceList } from "@/components/Evidence"
import { Markdown } from "@/components/Markdown"
import { Button } from "@/components/ui/button"
import { Panel, PanelHeader } from "@/components/ui/surface"
import { tools as toolsApi } from "@/lib/api"
import type { SkillDoc } from "@/lib/types"
import { formatRelative } from "@/lib/utils"
import { useApp } from "@/state/AppState"

export function SkillDetail() {
  const { skillId } = useParams()
  const { skills, mode, companyName } = useApp()
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
        <TrySkill skill={skill} company={companyName} />
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


/**
 * Hands the owner something to paste, then says whether it landed.
 *
 * There is no deep link that runs a prompt in Claude or Codex, so the
 * honest version of "try this" is a prepared question and the clipboard.
 * What makes it worth more than a copy button is the second half: the
 * gateway records every read, so the screen can wait and then name this
 * skill as the thing that was opened.
 *
 * Deliberately narrow. It reports a read of *this* skill, not merely that
 * something was served — "Claude read something" while the agent was in
 * fact answering from its own head is exactly the reassurance this
 * product exists to refuse.
 */
function TrySkill({ skill, company }: { skill: SkillDoc; company: string }) {
  const [copied, setCopied] = useState(false)
  const [watching, setWatching] = useState(false)
  const [readBy, setReadBy] = useState<string | null>(null)
  const since = useRef<string | null>(null)

  const question = `Walk me through ${skill.name.toLowerCase()}, the way ${company} actually does it.`

  useEffect(() => {
    if (!watching) return
    let cancelled = false

    const poll = async () => {
      const usage = await toolsApi.usage()
      if (cancelled) return
      // Anchored to the newest read at the moment it was copied, so a
      // read from yesterday cannot be mistaken for this one.
      const landed = usage.find(
        (row) =>
          (since.current === null || row.at > since.current) &&
          row.read.some((object) => object.id === skill.id),
      )
      if (landed) {
        setReadBy(landed.appLabel)
        setWatching(false)
      }
    }

    void poll()
    const timer = window.setInterval(poll, 2000)
    // Given up on rather than spun forever: somebody who copied this and
    // went to lunch should not come back to a screen still claiming to
    // be waiting.
    const giveUp = window.setTimeout(() => setWatching(false), 120_000)
    return () => {
      cancelled = true
      window.clearInterval(timer)
      window.clearTimeout(giveUp)
    }
  }, [watching, skill.id])

  const start = async () => {
    const usage = await toolsApi.usage()
    since.current = usage[0]?.at ?? null
    setReadBy(null)
    try {
      await navigator.clipboard.writeText(question)
      setCopied(true)
      setWatching(true)
    } catch {
      // A browser that refuses the clipboard still leaves the question
      // on screen to copy by hand, so this is not a dead end.
      setCopied(false)
      setWatching(true)
    }
  }

  return (
    <div className="flex min-w-[260px] flex-col items-end gap-2">
      <Button variant="primary" onClick={() => void start()}>
        {copied ? <Check /> : <Play />}
        {copied ? "Copied — paste it into your AI tool" : "Try this skill"}
      </Button>

      {copied || watching ? (
        <p className="max-w-[320px] rounded-md border border-line bg-surface-2 px-2.5 py-2 text-right text-[12px] text-muted">
          {question}
        </p>
      ) : null}

      {readBy ? (
        <p className="flex items-center gap-1.5 text-[12px] text-confirmed">
          <Check className="size-3.5" />
          {readBy} read this skill.
        </p>
      ) : watching ? (
        <p className="flex items-center gap-1.5 text-[12px] text-faint">
          <Loader2 className="size-3.5 animate-spin" />
          Waiting for a tool to open it…
        </p>
      ) : copied ? (
        <p className="flex items-center gap-1.5 text-[12px] text-faint">
          <Copy className="size-3.5" />
          Nothing opened it in two minutes.
        </p>
      ) : null}
    </div>
  )
}
