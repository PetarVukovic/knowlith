import { createContext, useCallback, useContext, useEffect, useMemo, useState } from "react"
import type { ReactNode } from "react"
import { api, failed as apiFailed, folders, usingDaemon } from "@/lib/api"
import type {
  CompilerRun,
  ContextObject,
  DiscoverySummary,
  MergeHint,
  ReviewItem,
  SkillDoc,
  Source,
  SourceDocument,
  ToolRead,
} from "@/lib/types"

type Activity = Awaited<ReturnType<typeof api.getRecentActivity>>

export type Theme = "light" | "dark" | "system"
/** Simple hides everything a non-technical owner should never have to see. */
export type UiMode = "simple" | "engineer"
/**
 * Where the owner is in their first run. Onboarding does not end when the
 * wizard closes: the first review and the first connected tool are part of it,
 * and they happen inside the real screens rather than in a simulation.
 */
export type FirstRun = "review" | "connect" | null

interface AppState {
  ready: boolean
  theme: Theme
  setTheme: (t: Theme) => void
  resolvedTheme: "light" | "dark"
  mode: UiMode
  setMode: (m: UiMode) => void
  onboarded: boolean
  completeOnboarding: () => void
  resetOnboarding: () => void
  firstRun: FirstRun
  setFirstRun: (stage: FirstRun) => void

  companyName: string
  setCompany: (name: string, logo: string | null) => void
  /** Data URL of the uploaded mark, or null for initials. */
  companyLogo: string | null
  objects: ContextObject[]
  review: ReviewItem[]
  sources: Source[]
  skills: SkillDoc[]
  discovery: DiscoverySummary | null
  runs: CompilerRun[]
  documents: SourceDocument[]
  toolReads: Record<string, ToolRead[]>
  activity: Activity
  /** Pairs the daemon cannot decide about, waiting for one human answer. */
  mergeHints: MergeHint[]
  /** What the background worker still has in hand. */
  work: { queued: number; working: number }

  approve: (itemId: string, edited: boolean) => void
  reject: (itemId: string) => void
  mergeObjects: (keepId: string, dropId: string) => void
  keepBoth: (keepId: string, dropId: string) => void
  setSourceStatus: (id: string, status: Source["status"]) => void
  removeSource: (id: string) => void
  /** Adds a folder and queues the walk. Resolves to a reason when it failed. */
  addSource: (path: string, name?: string, processor?: string) => Promise<Source[] | string>
  /** Pulls everything from the daemon again. */
  refresh: () => Promise<void>

  paletteOpen: boolean
  setPaletteOpen: (open: boolean) => void
  /** True when the screens are showing a real lake rather than the demo. */
  live: boolean
}

const Ctx = createContext<AppState | null>(null)

function readStored<T extends string>(key: string, fallback: T): T {
  try {
    return (localStorage.getItem(key) as T) ?? fallback
  } catch {
    return fallback
  }
}

function store(key: string, value: string) {
  try {
    localStorage.setItem(key, value)
  } catch {
    /* private window or blocked storage — the UI still works */
  }
}

export function AppProvider({ children }: { children: ReactNode }) {
  const [ready, setReady] = useState(false)
  const [theme, setThemeState] = useState<Theme>(() => readStored<Theme>("knowlith.theme", "system"))
  const [mode, setModeState] = useState<UiMode>(() => readStored<UiMode>("knowlith.mode", "simple"))
  const [onboarded, setOnboarded] = useState(() => readStored<"yes" | "no">("knowlith.onboarded", "no") === "yes")
  const [firstRun, setFirstRunState] = useState<FirstRun>(() => {
    const stored = readStored<string>("knowlith.firstRun", "")
    return stored === "review" || stored === "connect" ? stored : null
  })
  const [systemDark, setSystemDark] = useState(
    () => typeof window !== "undefined" && window.matchMedia("(prefers-color-scheme: dark)").matches,
  )

  // Empty until the daemon says who this is. A default here was a demo
  // company's name appearing on a real install for as long as the first
  // request took, and staying there whenever that request failed.
  const [companyName, setCompanyName] = useState("")
  const [companyLogo, setCompanyLogo] = useState<string | null>(null)
  const [objects, setObjects] = useState<ContextObject[]>([])
  const [review, setReview] = useState<ReviewItem[]>([])
  const [sources, setSources] = useState<Source[]>([])
  const [skills, setSkills] = useState<SkillDoc[]>([])
  const [discovery, setDiscovery] = useState<DiscoverySummary | null>(null)
  const [runs, setRuns] = useState<CompilerRun[]>([])
  const [documents, setDocuments] = useState<SourceDocument[]>([])
  const [toolReads, setToolReads] = useState<Record<string, ToolRead[]>>({})
  const [activity, setActivity] = useState<Activity>([])
  const [mergeHints, setMergeHints] = useState<MergeHint[]>([])
  const [work, setWork] = useState({ queued: 0, working: 0 })
  const [paletteOpen, setPaletteOpen] = useState(false)
  const [live, setLive] = useState(false)
  /** The last counts the daemon reported, so a change can be noticed. */
  const [, setSeen] = useState({ documents: -1, objects: -1 })

  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)")
    const onChange = (e: MediaQueryListEvent) => setSystemDark(e.matches)
    mq.addEventListener("change", onChange)
    return () => mq.removeEventListener("change", onChange)
  }, [])

  const resolvedTheme: "light" | "dark" = theme === "system" ? (systemDark ? "dark" : "light") : theme

  useEffect(() => {
    document.documentElement.classList.toggle("dark", resolvedTheme === "dark")
    document.documentElement.style.colorScheme = resolvedTheme
  }, [resolvedTheme])

  useEffect(() => {
    let cancelled = false
    void (async () => {
      const [c, o, r, s, sk, d, ru, docs, reads, act, hints] = await Promise.all([
        api.getCompany(),
        api.getObjects(),
        api.getReviewQueue(),
        api.getSources(),
        api.getSkills(),
        api.getDiscovery(),
        api.getCompilerRuns(),
        api.getSourceDocuments(),
        api.getToolReads(),
        api.getRecentActivity(),
        api.getMergeHints(),
      ])
      if (cancelled) return
      // Whatever the daemon says, including a stand-in it worked out from
      // the folder name. Never overwritten with a literal from this file.
      if (c.name.trim()) setCompanyName(c.name)
      setObjects(o)
      setReview(r)
      setSources(s)
      setSkills(sk)
      setDiscovery(d)
      setRuns(ru)
      setDocuments(docs)
      setToolReads(reads)
      setActivity(act)
      setMergeHints(hints)
      const daemon = await usingDaemon()
      setLive(daemon)
      // A wiped lake with a leftover "onboarded" flag used to drop the owner
      // on an empty Home. The wizard is the only honest screen until a folder
      // has actually been attached.
      if (daemon && s.length === 0) {
        setOnboarded(false)
        store("knowlith.onboarded", "no")
        setFirstRunState(null)
        store("knowlith.firstRun", "")
      }
      setReady(true)
    })()
    return () => {
      cancelled = true
    }
  }, [])

  /**
   * Watch the background worker while it has something in hand.
   *
   * Polled rather than pushed: a websocket for one number on one machine is
   * ceremony, and the owner needs to know the difference between "nothing is
   * happening" and "nothing is happening yet", which one number answers.
   */
  useEffect(() => {
    if (!live) return
    let cancelled = false
    const poll = async () => {
      const next = await api.getWork()
      if (cancelled) return
      // A fresh object every four seconds is a new context value every
      // four seconds, and every screen under it re-renders for a queue
      // that has not moved.
      setWork((current) =>
        current.queued === next.queued && current.working === next.working
          ? current
          : { queued: next.queued, working: next.working },
      )
      setSeen((current) => {
        // Watching the queue drain is not enough. With recorded replies the
        // whole of a small company compiles between two polls, and then the
        // queue is empty at both ends and the screens stay at zero forever.
        // What is watched instead is what came out: when the number of
        // documents or objects has moved, there is something new to show.
        if (current.documents === next.documents && current.objects === next.objects) {
          return current
        }
        {
          void (async () => {
            const [r, o, d, hints, sk, s, docs] = await Promise.all([
              api.getReviewQueue(),
              api.getObjects(),
              api.getDiscovery(),
              api.getMergeHints(),
              api.getSkills(),
              api.getSources(),
              api.getSourceDocuments(),
            ])
            if (cancelled) return
            setReview(r)
            setObjects(o)
            setDiscovery(d)
            setMergeHints(hints)
            setSkills(sk)
            setSources(s)
            setDocuments(docs)
          })()
        }
        return { documents: next.documents, objects: next.objects }
      })
    }
    void poll()
    const timer = window.setInterval(poll, 4000)
    return () => {
      cancelled = true
      window.clearInterval(timer)
    }
  }, [live])

  const setTheme = useCallback((t: Theme) => {
    setThemeState(t)
    store("knowlith.theme", t)
  }, [])

  const setMode = useCallback((m: UiMode) => {
    setModeState(m)
    store("knowlith.mode", m)
  }, [])

  const setFirstRun = useCallback((stage: FirstRun) => {
    setFirstRunState(stage)
    store("knowlith.firstRun", stage ?? "")
  }, [])

  const setCompany = useCallback((name: string, logo: string | null) => {
    setCompanyName(name)
    setCompanyLogo(logo)
    // Named in the lake as well as on screen, so the gateway, the Claude
    // Desktop extension and the standing instructions all say the same
    // thing without the daemon being restarted.
    void api.setCompanyName(name)
  }, [])

  const completeOnboarding = useCallback(() => {
    setOnboarded(true)
    store("knowlith.onboarded", "yes")
  }, [])

  const resetOnboarding = useCallback(() => {
    setOnboarded(false)
    store("knowlith.onboarded", "no")
  }, [])

  const approve = useCallback((itemId: string, edited: boolean) => {
    // Tell the daemon, then move the interface. The optimistic update is
    // what keeps the queue feeling immediate; a failed write shows up on the
    // next load rather than being invented here.
    //
    // And then ask again. Approving marks whatever rested on the old
    // wording as needing attention, which changes what is waiting without
    // changing how many objects exist — and the poll below only looks at
    // counts, so those new items would never arrive on their own.
    void api.approve(itemId, edited).then(async (affected) => {
      if (affected.length === 0) return
      const [queue, all] = await Promise.all([api.getReviewQueue(), api.getObjects()])
      setReview(queue)
      setObjects(all)
    })
    setReview((queue) => {
      const item = queue.find((i) => i.id === itemId)
      if (item) {
        setObjects((current) =>
          current.map((o) =>
            o.id === item.objectId
              ? {
                  ...o,
                  status: "approved",
                  body: item.after,
                  version: o.version + 1,
                  updatedAt: new Date().toISOString(),
                  decidedBy: "You",
                  editedOnApproval: edited,
                }
              : o,
          ),
        )
      }
      return queue.filter((i) => i.id !== itemId)
    })
  }, [])

  const reject = useCallback((itemId: string) => {
    void api.reject(itemId)
    setReview((queue) => queue.filter((i) => i.id !== itemId))
  }, [])

  /**
   * Fold one object into another.
   *
   * Nothing is deleted: the daemon marks the dropped version superseded and
   * moves its quotes onto the one that survives, so a merge the owner
   * regrets still has something to go back to.
   */
  const mergeObjects = useCallback((keepId: string, dropId: string) => {
    void api.merge(keepId, dropId)
    setMergeHints((hints) => hints.filter((h) => h.keepId !== keepId || h.dropId !== dropId))
    setObjects((current) =>
      current.map((o) => (o.id === dropId ? { ...o, status: "superseded" as const } : o)),
    )
    setReview((queue) => queue.filter((item) => item.objectId !== dropId))
  }, [])

  /** They are different rules. Recorded, so the question is not asked again. */
  const keepBoth = useCallback((keepId: string, dropId: string) => {
    void api.dismissMerge(keepId, dropId)
    setMergeHints((hints) => hints.filter((h) => h.keepId !== keepId || h.dropId !== dropId))
  }, [])

  /**
   * The badge follows the daemon, not the click. Flipping the row locally and
   * telling the daemon nothing showed "Paused" over a folder the worker was
   * still walking; now the row changes only once the daemon has recorded it,
   * and "scanning" stays a local hint because the daemon has no such state.
   */
  const setSourceStatus = useCallback((id: string, status: Source["status"]) => {
    if (status === "scanning") {
      setSources((current) => current.map((s) => (s.id === id ? { ...s, status } : s)))
      return
    }
    void api.setSourcePaused(id, status === "paused").then((result) => {
      if (!result) return
      setSources((current) => current.map((s) => (s.id === id ? { ...s, status: result.status } : s)))
    })
  }, [])

  const removeSource = useCallback((id: string) => {
    void api.removeSource(id).then((result) => {
      if (!result) return
      setSources((current) => current.filter((s) => s.id !== id))
    })
  }, [])

  /**
   * Points Knowlith at a folder.
   *
   * The walk is queued rather than performed, so the list is refetched
   * immediately — the source appears with nothing in it yet, and fills in as
   * the worker gets through it. Showing it straight away is the point: the
   * owner just chose it, and a screen that stays empty until the first
   * document lands reads as a button that did nothing.
   */
  /**
   * Everything the daemon knows, again.
   *
   * The background poll notices a change within a few seconds, which is soon
   * enough while somebody is reading a screen and not soon enough when they
   * are being moved to a new one. A screen that knows it has just caused a
   * change asks for the new state rather than showing zeros until the next
   * tick catches up.
   */
  const refresh = useCallback(async () => {
    const [o, r, s, sk, d, ru, docs, hints] = await Promise.all([
      api.getObjects(),
      api.getReviewQueue(),
      api.getSources(),
      api.getSkills(),
      api.getDiscovery(),
      api.getCompilerRuns(),
      api.getSourceDocuments(),
      api.getMergeHints(),
    ])
    setObjects(o)
    setReview(r)
    setSources(s)
    setSkills(sk)
    setDiscovery(d)
    setRuns(ru)
    setDocuments(docs)
    setMergeHints(hints)
  }, [])

  const addSource = useCallback(async (path: string, name?: string, processor?: string) => {
    const result = await folders.add(path, name, processor)
    if (apiFailed(result)) return result.error
    const next = await api.getSources()
    setSources(next)
    return next
  }, [])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "k" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault()
        setPaletteOpen((open) => !open)
      }
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [])

  const value = useMemo<AppState>(
    () => ({
      ready,
      theme,
      setTheme,
      resolvedTheme,
      mode,
      setMode,
      onboarded,
      completeOnboarding,
      resetOnboarding,
      firstRun,
      setFirstRun,
      companyName: companyName || "Your company",
      setCompany,
      companyLogo,
      objects,
      review,
      sources,
      skills,
      discovery,
      runs,
      documents,
      toolReads,
      activity,
      mergeHints,
      work,
      approve,
      reject,
      mergeObjects,
      keepBoth,
      setSourceStatus,
      addSource,
      refresh,
      removeSource,
      paletteOpen,
      setPaletteOpen,
      live,
    }),
    [
      ready,
      theme,
      setTheme,
      resolvedTheme,
      mode,
      setMode,
      onboarded,
      completeOnboarding,
      resetOnboarding,
      firstRun,
      setFirstRun,
      companyName,
      setCompany,
      companyLogo,
      objects,
      review,
      sources,
      skills,
      discovery,
      runs,
      documents,
      toolReads,
      activity,
      mergeHints,
      work,
      approve,
      reject,
      mergeObjects,
      keepBoth,
      setSourceStatus,
      addSource,
      refresh,
      removeSource,
      paletteOpen,
      live,
    ],
  )

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>
}

// eslint-disable-next-line react-refresh/only-export-components
export function useApp(): AppState {
  const ctx = useContext(Ctx)
  if (!ctx) throw new Error("useApp must be used inside AppProvider")
  return ctx
}
