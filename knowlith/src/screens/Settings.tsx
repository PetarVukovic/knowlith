import { useEffect, useRef, useState } from "react"
import { useLocation, useNavigate } from "react-router-dom"
import { ExternalLink, FolderOpen, Laptop, Moon, Plug, Sun, Trash2 } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input, Textarea } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
import { Field, Panel, PanelHeader } from "@/components/ui/surface"
import { api, background } from "@/lib/api"
import type { AutostartState, Policy, PolicyState, Source } from "@/lib/types"
import { cn, formatBytes, formatCount, formatRelative } from "@/lib/utils"
import { useApp, type Theme, type UiMode } from "@/state/AppState"

const THEMES: { id: Theme; label: string; Icon: typeof Sun }[] = [
  { id: "light", label: "Light", Icon: Sun },
  { id: "dark", label: "Dark", Icon: Moon },
  { id: "system", label: "Match system", Icon: Laptop },
]

const PROCESSING: { id: Policy["processing"]; label: string; hint: string }[] = [
  {
    id: "automatic",
    label: "Automatic",
    hint: "Read folders as they change. The usual choice.",
  },
  {
    id: "ask",
    label: "Ask first",
    hint: "Queue the work and wait until you say go.",
  },
  {
    id: "manual",
    label: "Only when I start it",
    hint: "Nothing runs on its own.",
  },
]

/**
 * Owner preferences that are not a daily job.
 *
 * Sources and setup live here together: remove folders you no longer want read,
 * then run the first-run wizard again in one pass. AI tools stay on Connect.
 */
export function Settings() {
  const {
    companyName,
    setCompany,
    theme,
    setTheme,
    mode,
    setMode,
    live,
    sources,
    removeSource,
    beginSetupAgain,
  } = useApp()
  const navigate = useNavigate()
  const location = useLocation()
  const setupRef = useRef<HTMLDivElement>(null)

  const [nameDraft, setNameDraft] = useState(companyName)
  const [profileDraft, setProfileDraft] = useState("")
  const [policyState, setPolicyState] = useState<PolicyState | null>(null)
  const [autostart, setAutostart] = useState<AutostartState | null>(null)
  const [saving, setSaving] = useState<string | null>(null)

  const [removeMarked, setRemoveMarked] = useState<Record<string, boolean>>({})
  const [pendingRemoval, setPendingRemoval] = useState<Source | null>(null)
  const [restartOpen, setRestartOpen] = useState(false)
  const [restarting, setRestarting] = useState(false)

  useEffect(() => {
    setNameDraft(companyName)
  }, [companyName])

  useEffect(() => {
    if (!live) return
    let cancelled = false
    void (async () => {
      const [p, a, c] = await Promise.all([
        background.policy(),
        background.autostart(),
        api.getCompany(),
      ])
      if (cancelled) return
      setPolicyState(p)
      setAutostart(a)
      setProfileDraft(c.profile ?? "")
    })()
    return () => {
      cancelled = true
    }
  }, [live])

  useEffect(() => {
    const scroll = (location.state as { scroll?: string } | null)?.scroll
    if (scroll === "setup") {
      setupRef.current?.scrollIntoView({ behavior: "smooth", block: "start" })
    }
  }, [location.state])

  const markedIds = sources.filter((s) => removeMarked[s.id]).map((s) => s.id)

  const saveName = () => {
    const next = nameDraft.trim()
    if (!next || next === companyName) return
    setCompany(next, null)
  }

  const saveProfile = async () => {
    setSaving("profile")
    await api.setCompanyProfile(profileDraft)
    setSaving(null)
  }

  const patchPolicy = async (patch: Partial<Policy>) => {
    if (!policyState) return
    setSaving("policy")
    const next = {
      ...policyState.policy,
      engine: policyState.policy.engine || "auto",
      compileWorkers: policyState.policy.compileWorkers ?? 2,
      compileBatchSize: policyState.policy.compileBatchSize ?? 8,
      ...patch,
    }
    const saved = await background.setPolicy(next)
    setPolicyState(saved)
    setSaving(null)
  }

  const toggleAutostart = async (on: boolean) => {
    setSaving("autostart")
    const saved = await background.setAutostart(on)
    if (saved) setAutostart(saved)
    else {
      const again = await background.autostart()
      setAutostart(again)
    }
    setSaving(null)
  }

  const runSetupAgain = async () => {
    setRestarting(true)
    await beginSetupAgain(markedIds)
    setRestarting(false)
    setRestartOpen(false)
    navigate("/onboarding")
  }

  return (
    <div className="mx-auto w-full max-w-[640px] px-4 py-8">
      <h1 className="text-[20px] font-semibold tracking-[-0.015em] text-ink">Settings</h1>
      <p className="mt-1 max-w-[54ch] text-[13px] text-muted">
        How Knowlith looks, which folders it reads, and when it may read them.
      </p>

      <div className="mt-6 grid gap-4">
        <Panel>
          <PanelHeader title="Company" description="Shown in the app and to your AI tools." />
          <div className="grid gap-3 p-4">
            <Field label="Company name" hint="Use the name your team already uses.">
              <div className="flex flex-wrap gap-2">
                <Input
                  value={nameDraft}
                  onChange={(e) => setNameDraft(e.target.value)}
                  onBlur={saveName}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.currentTarget.blur()
                    }
                  }}
                  className="max-w-[320px]"
                />
                <Button
                  variant="default"
                  size="sm"
                  disabled={!nameDraft.trim() || nameDraft.trim() === companyName}
                  onClick={saveName}
                >
                  Save
                </Button>
              </div>
            </Field>
            <Field
              label="What this company is"
              hint="A few sentences: industry, customers, what you sell. The compiler uses this so it does not invent invoice schemas from a folder of invoices."
            >
              <Textarea
                value={profileDraft}
                onChange={(e) => setProfileDraft(e.target.value)}
                rows={4}
                className="font-sans text-[13.5px]"
                placeholder="e.g. HVAC installer for Croatian SMBs. Price lists and payment terms matter; individual invoices usually do not."
              />
              <Button
                className="mt-2"
                variant="default"
                size="sm"
                disabled={saving === "profile"}
                onClick={() => void saveProfile()}
              >
                {saving === "profile" ? "Saving…" : "Save profile"}
              </Button>
            </Field>
          </div>
        </Panel>

        <div ref={setupRef} id="sources-and-setup">
        <Panel>
          <PanelHeader
            title="Sources and setup"
            description="Stop reading folders you no longer want, then walk through setup again — in one go."
          />
          <div className="grid gap-4 p-4">
            {!live ? (
              <p className="text-[13px] text-faint">Start Knowlith to change which folders are read.</p>
            ) : sources.length === 0 ? (
              <div className="rounded-md border border-line bg-surface-2 px-3 py-3 text-[13px] text-muted">
                No folders yet. Run setup to add the documents Knowlith should read.
              </div>
            ) : (
              <ul className="grid gap-2">
                {sources.map((source) => (
                  <li
                    key={source.id}
                    className="flex items-start gap-3 rounded-md border border-line px-3 py-2.5"
                  >
                    <label className="mt-0.5 flex shrink-0 cursor-pointer items-center">
                      <input
                        type="checkbox"
                        checked={removeMarked[source.id] ?? false}
                        onChange={(e) =>
                          setRemoveMarked((current) => ({ ...current, [source.id]: e.target.checked }))
                        }
                        className="size-3.5 rounded border-line accent-accent"
                        aria-label={`Stop reading ${source.name}`}
                      />
                    </label>
                    <div className="min-w-0 flex-1">
                      <div className="text-[13.5px] font-medium text-ink">{source.name}</div>
                      <div className="mt-0.5 text-[12.5px] text-muted">
                        {formatCount(source.fileCount)} files · {formatBytes(source.bytes)}
                        {source.lastAnalyzed ? ` · last read ${formatRelative(source.lastAnalyzed)}` : " · not read yet"}
                      </div>
                      {mode === "engineer" ? (
                        <div className="mt-0.5 truncate font-mono text-[11.5px] text-faint">{source.path}</div>
                      ) : null}
                    </div>
                    <Button
                      size="icon-sm"
                      variant="ghost"
                      aria-label={`Remove ${source.name}`}
                      onClick={() => setPendingRemoval(source)}
                    >
                      <Trash2 className="size-3.5 text-faint" />
                    </Button>
                  </li>
                ))}
              </ul>
            )}

            <p className="text-[12.5px] leading-relaxed text-muted">
              Checked folders stop being read. Your files on disk are untouched. Rules you already approved
              stay in use — they just will not update from a removed folder.
            </p>

            <div className="flex flex-wrap items-center gap-2">
              <Button
                variant="primary"
                disabled={!live || restarting}
                onClick={() => setRestartOpen(true)}
              >
                Run setup again
              </Button>
              <Button variant="ghost" size="sm" onClick={() => navigate("/sources")}>
                <FolderOpen className="size-3.5" />
                Pause, rescan, or add folders
                <ExternalLink className="size-3 text-faint" />
              </Button>
            </div>
          </div>
        </Panel>
        </div>

        <Panel>
          <PanelHeader title="Appearance" description="Light or dark. Does not change what is stored." />
          <div className="flex flex-wrap gap-2 p-4">
            {THEMES.map(({ id, label, Icon }) => (
              <button
                key={id}
                type="button"
                onClick={() => setTheme(id)}
                className={cn(
                  "inline-flex items-center gap-2 rounded-md border px-3 py-2 text-[13px]",
                  theme === id
                    ? "border-accent bg-accent-soft font-medium text-accent"
                    : "border-line text-muted hover:border-line-strong hover:text-ink",
                )}
              >
                <Icon className="size-3.5" />
                {label}
              </button>
            ))}
          </div>
        </Panel>

        <Panel>
          <PanelHeader
            title="Detail level"
            description="Simple is for everyday use. Engineer shows paths, ids and raw data."
          />
          <div className="grid gap-2 p-4">
            {(
              [
                { id: "simple" as UiMode, label: "Simple", hint: "Claims, quotes, and what needs your OK." },
                {
                  id: "engineer" as UiMode,
                  label: "Engineer",
                  hint: "Paths, object ids, byte offsets, compile stats.",
                },
              ] as const
            ).map((opt) => (
              <button
                key={opt.id}
                type="button"
                onClick={() => setMode(opt.id)}
                className={cn(
                  "rounded-md border px-3 py-2.5 text-left",
                  mode === opt.id
                    ? "border-accent bg-accent-soft"
                    : "border-line hover:border-line-strong",
                )}
              >
                <span className={cn("block text-[13.5px] font-medium", mode === opt.id ? "text-accent" : "text-ink")}>
                  {opt.label}
                </span>
                <span className="mt-0.5 block text-[12.5px] text-muted">{opt.hint}</span>
              </button>
            ))}
          </div>
        </Panel>

        <Panel>
          <PanelHeader
            title="When folders are read"
            description={
              live
                ? policyState?.onBattery
                  ? "This Mac is on battery right now."
                  : "Controls the background worker on this Mac."
                : "Start Knowlith to change these."
            }
          />
          <div className="grid gap-4 p-4">
            {!live || !policyState ? (
              <p className="text-[13px] text-faint">
                {live ? "Could not load reading policy." : "Knowlith is not running."}
              </p>
            ) : (
              <>
                <div className="grid gap-2">
                  {PROCESSING.map((opt) => (
                    <button
                      key={opt.id}
                      type="button"
                      disabled={saving === "policy"}
                      onClick={() => void patchPolicy({ processing: opt.id })}
                      className={cn(
                        "rounded-md border px-3 py-2.5 text-left",
                        policyState.policy.processing === opt.id
                          ? "border-accent bg-accent-soft"
                          : "border-line hover:border-line-strong",
                      )}
                    >
                      <span
                        className={cn(
                          "block text-[13.5px] font-medium",
                          policyState.policy.processing === opt.id ? "text-accent" : "text-ink",
                        )}
                      >
                        {opt.label}
                      </span>
                      <span className="mt-0.5 block text-[12.5px] text-muted">{opt.hint}</span>
                    </button>
                  ))}
                </div>

                <label className="flex items-start justify-between gap-4 rounded-md border border-line px-3 py-2.5">
                  <span>
                    <span className="block text-[13.5px] font-medium text-ink">Pause AI work on battery</span>
                    <span className="mt-0.5 block text-[12.5px] text-muted">
                      Folders can still be listed. Asking a model waits until you plug in.
                    </span>
                  </span>
                  <Switch
                    checked={policyState.policy.pauseOnBattery}
                    disabled={saving === "policy"}
                    onCheckedChange={(checked) => void patchPolicy({ pauseOnBattery: checked })}
                    aria-label="Pause AI work on battery"
                  />
                </label>

                <div className="grid gap-2 border-t border-line pt-4">
                  <div className="text-[13px] font-medium text-ink">Who reads your documents</div>
                  <p className="text-[12.5px] text-muted">
                    Codex, Claude Code or Cursor Agent on this Mac. Changing this needs a
                    Knowlith restart to take effect on work already queued.
                  </p>
                  {(
                    [
                      {
                        id: "auto",
                        label: "Automatic",
                        hint: "Whatever is installed (Claude → Codex → Cursor Agent).",
                      },
                      { id: "codex", label: "Codex", hint: "Your Codex CLI on this Mac." },
                      { id: "claude-code", label: "Claude Code", hint: "Your Claude CLI on this Mac." },
                      {
                        id: "cursor-agent",
                        label: "Cursor Agent",
                        hint: "Your `agent` CLI on this Mac.",
                      },
                    ] as const
                  ).map((opt) => (
                    <button
                      key={opt.id}
                      type="button"
                      disabled={saving === "policy"}
                      onClick={() => void patchPolicy({ engine: opt.id })}
                      className={cn(
                        "rounded-md border px-3 py-2.5 text-left",
                        (policyState.policy.engine || "auto") === opt.id
                          ? "border-accent bg-accent-soft"
                          : "border-line hover:border-line-strong",
                      )}
                    >
                      <span
                        className={cn(
                          "block text-[13.5px] font-medium",
                          (policyState.policy.engine || "auto") === opt.id ? "text-accent" : "text-ink",
                        )}
                      >
                        {opt.label}
                      </span>
                      <span className="mt-0.5 block text-[12.5px] text-muted">{opt.hint}</span>
                    </button>
                  ))}
                  {policyState.engineRestart ? (
                    <p className="rounded-md border border-line bg-surface-2 px-3 py-2 text-[12.5px] text-muted">
                      {policyState.engineRestart}
                    </p>
                  ) : null}
                </div>

                <Field
                  label="Ask before a large first read"
                  hint="How many new or changed files count as large. Default is 500."
                >
                  <Input
                    type="number"
                    min={50}
                    max={50000}
                    step={50}
                    className="max-w-[140px]"
                    value={policyState.policy.largeScan}
                    disabled={saving === "policy"}
                    onChange={(e) => {
                      const n = Number(e.target.value)
                      if (!Number.isFinite(n)) return
                      setPolicyState({
                        ...policyState,
                        policy: { ...policyState.policy, largeScan: n },
                      })
                    }}
                    onBlur={() => {
                      const n = Math.max(50, Math.min(50000, Math.round(policyState.policy.largeScan)))
                      void patchPolicy({ largeScan: n })
                    }}
                  />
                </Field>

                <Field
                  label="CLI workers"
                  hint="Parallel compile workers beside the folder walk. Restart Knowlith after changing. Default 2, max 4."
                >
                  <Input
                    type="number"
                    min={1}
                    max={4}
                    step={1}
                    className="max-w-[140px]"
                    value={policyState.policy.compileWorkers ?? 2}
                    disabled={saving === "policy"}
                    onChange={(e) => {
                      const n = Number(e.target.value)
                      if (!Number.isFinite(n)) return
                      setPolicyState({
                        ...policyState,
                        policy: { ...policyState.policy, compileWorkers: n },
                      })
                    }}
                    onBlur={() => {
                      const n = Math.max(1, Math.min(4, Math.round(policyState.policy.compileWorkers ?? 2)))
                      void patchPolicy({ compileWorkers: n })
                    }}
                  />
                </Field>

                <Field
                  label="Documents per CLI run"
                  hint="How many files one Codex / Claude / Cursor process reads together. Higher = fewer cold starts. Default 8."
                >
                  <Input
                    type="number"
                    min={1}
                    max={20}
                    step={1}
                    className="max-w-[140px]"
                    value={policyState.policy.compileBatchSize ?? 8}
                    disabled={saving === "policy"}
                    onChange={(e) => {
                      const n = Number(e.target.value)
                      if (!Number.isFinite(n)) return
                      setPolicyState({
                        ...policyState,
                        policy: { ...policyState.policy, compileBatchSize: n },
                      })
                    }}
                    onBlur={() => {
                      const n = Math.max(
                        1,
                        Math.min(20, Math.round(policyState.policy.compileBatchSize ?? 8)),
                      )
                      void patchPolicy({ compileBatchSize: n })
                    }}
                  />
                </Field>

                {policyState.held.length > 0 ? (
                  <p className="rounded-md border border-pending/30 bg-pending-soft px-3 py-2 text-[12.5px] text-pending">
                    {policyState.held.reduce((n, h) => n + h.count, 0)} jobs are held right now
                    {policyState.onBattery ? " (battery)" : ""}.
                  </p>
                ) : null}
              </>
            )}
          </div>
        </Panel>

        <Panel>
          <PanelHeader title="Start with this Mac" description="So folders keep being read when you log in." />
          <div className="p-4">
            {!live || !autostart ? (
              <p className="text-[13px] text-faint">
                {live ? "Could not read login-item status." : "Knowlith is not running."}
              </p>
            ) : (
              <label className="flex items-start justify-between gap-4">
                <span>
                  <span className="block text-[13.5px] font-medium text-ink">Open Knowlith at login</span>
                  {autostart.location ? (
                    <span className="mt-0.5 block font-mono text-[11.5px] text-faint">{autostart.location}</span>
                  ) : (
                    <span className="mt-0.5 block text-[12.5px] text-muted">Uses this Mac’s login items.</span>
                  )}
                </span>
                <Switch
                  checked={autostart.enabled}
                  disabled={saving === "autostart"}
                  onCheckedChange={(checked) => void toggleAutostart(checked)}
                  aria-label="Open Knowlith at login"
                />
              </label>
            )}
          </div>
        </Panel>

        <Panel>
          <PanelHeader title="Related" description="Managed on their own screens." />
          <div className="grid gap-1 p-2">
            <RelatedLink
              icon={<Plug className="size-3.5" />}
              label="AI assistants"
              hint="Claude, Codex, Cursor, and who has used what"
              onClick={() => navigate("/connect")}
            />
          </div>
        </Panel>
      </div>

      <Dialog open={pendingRemoval !== null} onOpenChange={(open) => !open && setPendingRemoval(null)}>
        <DialogContent>
          <DialogTitle className="text-[15px] font-semibold text-ink">
            Remove {pendingRemoval?.name}?
          </DialogTitle>
          <DialogDescription className="mt-1.5 text-[13px] text-muted">
            Knowlith stops reading this folder. Your files are untouched. Context already approved from it stays in
            use, but it will no longer update.
          </DialogDescription>
          <div className="mt-5 flex justify-end gap-2">
            <DialogClose asChild>
              <Button variant="ghost">Keep it</Button>
            </DialogClose>
            <Button
              variant="danger"
              onClick={() => {
                if (pendingRemoval) {
                  removeSource(pendingRemoval.id)
                  setRemoveMarked((current) => {
                    const next = { ...current }
                    delete next[pendingRemoval.id]
                    return next
                  })
                }
                setPendingRemoval(null)
              }}
            >
              Remove source
            </Button>
          </div>
        </DialogContent>
      </Dialog>

      <Dialog open={restartOpen} onOpenChange={(open) => !restarting && setRestartOpen(open)}>
        <DialogContent>
          <DialogTitle className="text-[15px] font-semibold text-ink">Run setup again?</DialogTitle>
          <DialogDescription className="mt-1.5 text-[13px] text-muted">
            {markedIds.length > 0 ? (
              <>
                Knowlith will stop reading {markedIds.length}{" "}
                {markedIds.length === 1 ? "folder" : "folders"}, then walk you through setup from the welcome
                screen. Approved knowledge stays — only what still has a live source keeps updating.
              </>
            ) : sources.length > 0 ? (
              <>
                You will go through setup from the welcome screen. Folders Knowlith already reads stay attached;
                check any you want removed before continuing.
              </>
            ) : (
              <>You will go through setup from the welcome screen and add folders again.</>
            )}
          </DialogDescription>
          <div className="mt-5 flex justify-end gap-2">
            <DialogClose asChild>
              <Button variant="ghost" disabled={restarting}>
                Cancel
              </Button>
            </DialogClose>
            <Button variant="primary" disabled={restarting} onClick={() => void runSetupAgain()}>
              {restarting
                ? "Starting…"
                : markedIds.length > 0
                  ? `Remove ${markedIds.length} and continue`
                  : "Continue to setup"}
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  )
}

function RelatedLink({
  icon,
  label,
  hint,
  onClick,
}: {
  icon: React.ReactNode
  label: string
  hint: string
  onClick: () => void
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="flex w-full items-center gap-3 rounded-md px-2.5 py-2.5 text-left hover:bg-surface-2"
    >
      <span className="grid size-7 place-items-center rounded-md border border-line bg-surface-2 text-faint">
        {icon}
      </span>
      <span className="min-w-0 flex-1">
        <span className="block text-[13.5px] font-medium text-ink">{label}</span>
        <span className="block text-[12.5px] text-muted">{hint}</span>
      </span>
    </button>
  )
}
