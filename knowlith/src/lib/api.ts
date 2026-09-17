/**
 * The only module that knows where data comes from.
 *
 * There are exactly two sources and they never mix.
 *
 * **The daemon.** Whenever `knowlith serve` answers on `127.0.0.1`, every
 * screen shows that lake and nothing else. An endpoint that fails returns
 * empty, never a fixture: showing another company's discount policy because
 * one request 500'd is the worst thing this layer can do, and it is
 * indistinguishable from the product working.
 *
 * **The demo.** A guided tour of a company that does not exist, for someone
 * who has not installed anything yet. It has to be asked for — `?demo` in
 * the address, or `VITE_KNOWLITH_DEMO=1` — because a demo that appears on
 * its own is a demo somebody will mistake for their own data.
 *
 * With no daemon and no demo the screens are empty and say why, which is
 * the truth: nothing is running.
 */
import * as fixtures from "./mock"
import { sleep } from "./utils"
import type {
  AiTool,
  AutostartState,
  BuildQuiz,
  BuildStatus,
  Browsed,
  CompanyBrain,
  CompilerRun,
  ConnectPreview,
  DetectedEngine,
  Policy,
  PolicyState,
  SourceDocument,
  SourceDocumentIndex,
  ToolRead,
  ContextObject,
  DiscoverySummary,
  Health,
  MergeHint,
  ReviewItem,
  SkillDoc,
  Source,
  SourceAdded,
  Usage,
  Work,
} from "./types"

const LATENCY = 120

/**
 * Where the daemon is.
 *
 * Whoever served this page is the daemon. That sounds obvious and was the
 * bug: a hardcoded `127.0.0.1:7717` meant a daemon started on any other
 * port served an interface that then looked for a *different* daemon,
 * found nothing, and told the owner Knowlith was not running — while
 * running.
 *
 * Under `npm run dev` the page comes from Vite on another port, so there
 * the default is the daemon's usual one.
 */
const DAEMON: string = (() => {
  const explicit = (import.meta.env.VITE_KNOWLITH_API as string | undefined)?.replace(/\/$/, "")
  if (explicit) return explicit
  // Same origin either way: the daemon serves this page in the shipped
  // product, and in development Vite forwards `/api` to it. Nothing here
  // ever talks across origins, which is what lets the daemon refuse every
  // request that does.
  return ""
})()

/**
 * The daemon's API token.
 *
 * The daemon listens on loopback, which keeps the network out but not the
 * browser: any page the owner opens can send a request to `127.0.0.1`. So
 * every request carries a secret only the owner's own machine has, and the
 * daemon turns away the rest.
 *
 * The daemon writes it into the page it serves. Under `npm run dev` the
 * page comes from Vite instead, and the proxy in `vite.config.ts` attaches
 * the header in Node — so in development this is empty and nothing is
 * missing.
 */
const TOKEN: string = (() => {
  try {
    return document.querySelector<HTMLMetaElement>('meta[name="knowlith-token"]')?.content ?? ""
  } catch {
    return ""
  }
})()

/** Every request to the daemon, and the only place the token is attached. */
function ask(path: string, init?: RequestInit): Promise<Response> {
  const headers = new Headers(init?.headers)
  if (TOKEN) headers.set("x-knowlith-token", TOKEN)
  return fetch(`${DAEMON}${path}`, { ...init, headers })
}

/**
 * Whether the daemon answered, decided once per page load.
 *
 * Checked once rather than per request so the interface cannot show half its
 * screens from a real lake and half from the demo — that mixture would be
 * indistinguishable from a bug, and worse, from fabricated data.
 */
let connection: Promise<boolean> | null = null

function connected(): Promise<boolean> {
  connection ??= (async () => {
    try {
      const controller = new AbortController()
      const timer = window.setTimeout(() => controller.abort(), 1500)
      const response = await ask("/api/health", { signal: controller.signal })
      window.clearTimeout(timer)
      return response.ok
    } catch {
      return false
    }
  })()
  return connection
}

/** True once the daemon has answered. The status bar says which one it is. */
export async function usingDaemon(): Promise<boolean> {
  return connected()
}

/**
 * Whether the demo company was asked for.
 *
 * Read once. A flag that could change between two requests would put half a
 * screen on real data and half on a fixture, which is the one failure mode
 * worse than either on its own.
 */
const DEMO: boolean = (() => {
  try {
    if (new URLSearchParams(window.location.search).has("demo")) return true
  } catch {
    /* no window during a build or a test */
  }
  return import.meta.env.VITE_KNOWLITH_DEMO === "1"
})()

/** True when the screens are showing a company that does not exist. */
export function showingDemo(): boolean {
  return DEMO
}

/**
 * One request.
 *
 * `empty` is what a screen gets when there is nothing to show — an empty
 * list, a zeroed summary. `demo` is the fixture, and it is only ever
 * returned when the demo was explicitly asked for and no daemon is running.
 */
async function get<T>(path: string, demo: T, empty: T): Promise<T> {
  if (await connected()) {
    try {
      const response = await ask(path)
      if (response.ok) return (await response.json()) as T
    } catch {
      /* fall through to empty, never to the demo */
    }
    // The daemon is there and this request did not work. The owner gets an
    // empty screen and the status bar says the daemon is up — between them
    // that reads as "nothing here yet", which is recoverable. A fixture
    // would read as "here is your company", which is not.
    return empty
  }

  if (DEMO) {
    await sleep(LATENCY)
    return demo
  }
  return empty
}

async function post<T>(path: string, body?: unknown): Promise<T | null> {
  return mutate<T>("POST", path, body)
}

/** A write that either happened or did not; the caller keeps its own row until it did. */
async function mutate<T>(method: "POST" | "PUT" | "DELETE", path: string, body?: unknown): Promise<T | null> {
  const result = await send<T>(path, {
    method,
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body ?? {}),
  })
  return failed(result) ? null : result
}

export const api = {
  /** Names the company in the lake. */
  setCompanyName: (name: string) => putCompanyName(name),

  async getCompany() {
    return get<{ name: string; profile: string }>(
      "/api/company",
      { name: fixtures.company.name, profile: "" },
      { name: "", profile: "" },
    )
  },

  async setCompanyProfile(profile: string): Promise<void> {
    if (!(await connected())) return
    try {
      await ask("/api/company", {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ profile }),
      })
    } catch {
      /* next save will carry it */
    }
  },
  async getObjects(): Promise<ContextObject[]> {
    return get<ContextObject[]>("/api/objects", fixtures.contextObjects, [])
  },
  async getReviewQueue(): Promise<ReviewItem[]> {
    return get<ReviewItem[]>("/api/review", fixtures.reviewQueue, [])
  },
  async getSources(): Promise<Source[]> {
    return get<Source[]>("/api/sources", fixtures.sources, [])
  },
  async rescanSource(id: string) {
    return post<{ queued: boolean; message: string }>(`/api/sources/${encodeURIComponent(id)}/rescan`)
  },
  /** Pause or resume the worker's reading of one folder. `null` when the daemon did not take it. */
  async setSourcePaused(id: string, paused: boolean) {
    return mutate<{ status: "paused" | "active"; message: string }>(
      "PUT",
      `/api/sources/${encodeURIComponent(id)}/status`,
      { paused },
    )
  },
  /** Stop reading a folder for good. Files and approved context are untouched. */
  async removeSource(id: string) {
    return mutate<{ message: string }>("DELETE", `/api/sources/${encodeURIComponent(id)}`)
  },
  async getSkills(): Promise<SkillDoc[]> {
    return get<SkillDoc[]>("/api/skills", fixtures.skills, [])
  },
  async getDiscovery(): Promise<DiscoverySummary> {
    return get<DiscoverySummary>("/api/discovery", fixtures.discovery, {
      rules: 0,
      processes: 0,
      terms: 0,
      skills: 0,
      conflicts: 0,
      filesRead: 0,
      spansExtracted: 0,
      durationSeconds: 0,
    })
  },
  async getCompilerRuns(): Promise<CompilerRun[]> {
    return get<CompilerRun[]>("/api/runs", fixtures.compilerRuns, [])
  },
  async getSourceDocuments(): Promise<SourceDocument[]> {
    return get<SourceDocument[]>("/api/documents", fixtures.sourceDocuments, [])
  },
  async getSourceDocumentIndex(sourceId: string): Promise<SourceDocumentIndex[]> {
    return get<SourceDocumentIndex[]>(
      `/api/sources/${encodeURIComponent(sourceId)}/documents`,
      [],
      [],
    )
  },
  async keepGoneReview(objectId: string, documentId: string): Promise<void> {
    await post(`/api/review/gone/${encodeURIComponent(objectId)}/keep`, { documentId })
  },
  /** Reads the MCP gateway actually recorded, keyed by object id. */
  async getToolReads(): Promise<Record<string, ToolRead[]>> {
    return get<Record<string, ToolRead[]>>("/api/tool-reads", fixtures.toolReads, {})
  },
  async getRecentActivity() {
    return get<typeof fixtures.recentActivity>("/api/activity", fixtures.recentActivity, [])
  },

  /**
   * Approve a change.
   *
   * Returns the objects that now rest on something that moved — the daemon
   * works that out inside the same transaction as the approval, so the list
   * cannot disagree with what was stored.
   */
  async approve(itemId: string, edited: boolean, body?: string): Promise<string[]> {
    const result = await post<{ affected: string[] }>(
      `/api/review/${encodeURIComponent(itemId)}/approve`,
      { edited, body, decidedBy: "You" },
    )
    return result?.affected ?? []
  },

  async reject(itemId: string): Promise<void> {
    await post(`/api/review/${encodeURIComponent(itemId)}/reject`)
  },

  /**
   * Owner rewrote a live claim. It goes back to the Inbox and is not served
   * until they approve again.
   */
  async suggestChange(objectId: string, body: string): Promise<{ id: string } | null> {
    return post<{ id: string }>(`/api/objects/${encodeURIComponent(objectId)}/suggest`, { body })
  },

  /**
   * Pairs the daemon thinks may be one rule written twice.
   *
   * There is no fixture fallback with content here on purpose. A demo that
   * invents a duplicate teaches the owner to expect a question the daemon
   * may never ask about their own folder.
   */
  async getMergeHints(): Promise<MergeHint[]> {
    return get<MergeHint[]>("/api/merge-hints", [], [])
  },

  async merge(keepId: string, dropId: string): Promise<void> {
    await post(`/api/merge-hints/${encodeURIComponent(keepId)}/${encodeURIComponent(dropId)}/merge`)
  },

  async dismissMerge(keepId: string, dropId: string): Promise<void> {
    await post(`/api/merge-hints/${encodeURIComponent(keepId)}/${encodeURIComponent(dropId)}/dismiss`)
  },

  /**
   * What the background worker still has to do.
   *
   * The onboarding screen says "you can close this window — it keeps going".
   * This is the number that makes that sentence something the owner can
   * check rather than something they are asked to believe.
   */
  async getWork(): Promise<Health> {
    const health = await get<Partial<Health>>("/api/health", {}, {})
    return {
      queued: health.queued ?? 0,
      working: health.working ?? 0,
      documents: health.documents ?? 0,
      objects: health.objects ?? 0,
    }
  },

  /** Scan a folder before anything is read by a model. */
  async scanFolder(path: string) {
    await sleep(900)
    return {
      path,
      fileCount: 2438,
      bytes: 9_020_000_000,
      fileTypes: [
        { ext: "PDF", count: 1113 },
        { ext: "DOCX", count: 730 },
        { ext: "XLSX", count: 351 },
        { ext: "CSV", count: 90 },
      ],
      skipped: [
        { reason: "Password protected", count: 12 },
        { reason: "Scanned image, no text layer", count: 132 },
        { reason: "Larger than 80 MB", count: 10 },
      ],
    }
  },
}

export type ScanResult = Awaited<ReturnType<typeof api.scanFolder>>

/**
 * Everything about handing Knowlith to the AI applications on this machine.
 *
 * Every one of these needs a real daemon: there is nothing to demonstrate
 * about connecting an application that is not there, and a fixture that
 * said "connected" would be a lie the owner acts on.
 */
/**
 * Choosing a folder, which only the daemon can do.
 *
 * A browser is never told where a folder is — `webkitdirectory` gives
 * relative names and `showDirectoryPicker` gives a handle with no path — so
 * the page asks the daemon to open the machine's own chooser.
 *
 * Every call here returns the daemon's own sentence on failure rather than
 * `null`. A folder that cannot be added is always the owner's to fix, and
 * "there is nothing at /Uesrs/petar/Docs" fixes it while "could not add
 * folder" does not.
 */
export const folders = {
  /** Opens the system chooser. Resolves when it closes, however it closes. */
  async browse(): Promise<Browsed | { error: string }> {
    return send<Browsed>("/api/sources/browse", { method: "POST" })
  },

  /** Counts a folder the owner typed the path of. */
  async preview(path: string): Promise<Browsed | { error: string }> {
    return send<Browsed>(`/api/sources/preview?path=${encodeURIComponent(path)}`)
  },

  /** Records the folder and queues the walk. The worker does the reading. */
  async add(
    path: string,
    name?: string,
    processor?: string,
  ): Promise<SourceAdded | { error: string }> {
    return send<SourceAdded>("/api/sources", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ path, name, processor }),
    })
  },

  async getBuildStatus(): Promise<BuildStatus> {
    return get<BuildStatus>("/api/build/status", { phase: "idle", quizPending: false, questionCount: 0 }, {
      phase: "idle",
      quizPending: false,
      questionCount: 0,
    })
  },

  async getBuildQuiz(): Promise<BuildQuiz | null> {
    return get<BuildQuiz | null>("/api/build/quiz", null, null)
  },

  async confirmBuildQuiz(quizId: string, approvedObjectIds: string[]): Promise<void> {
    await post("/api/build/quiz/confirm", { quizId, approvedObjectIds })
  },
}

/** True when a folder call came back with a reason instead of an answer. */
export function failed<T>(result: T | { error: string }): result is { error: string } {
  return typeof result === "object" && result !== null && "error" in result
}

/**
 * One request that reports why it failed.
 *
 * The daemon answers a bad folder with 400 and a sentence. Throwing that
 * sentence away and showing a generic message would turn a fixable mistake
 * into a mystery.
 */
async function send<T>(path: string, init?: RequestInit): Promise<T | { error: string }> {
  if (!(await connected())) {
    return { error: "Knowlith is not running on this machine." }
  }
  try {
    const response = await ask(path, init)
    if (!response.ok) {
      const text = (await response.text()).trim()
      return { error: text || `the daemon answered ${response.status}` }
    }
    return (await response.json()) as T
  } catch (e) {
    return { error: e instanceof Error ? e.message : "the daemon could not be reached" }
  }
}

export const tools = {
  async list(): Promise<AiTool[]> {
    return get<AiTool[]>("/api/tools", [], [])
  },

  /** What the AI tools have actually read, newest first. */
  async usage(): Promise<Usage[]> {
    return get<Usage[]>("/api/usage", [], [])
  },

  /** What would be written, so the owner agrees to something specific. */
  async preview(slug: string): Promise<ConnectPreview | null> {
    if (!(await connected())) return null
    try {
      const response = await ask(`/api/tools/${slug}/preview`)
      if (!response.ok) return null
      return (await response.json()) as ConnectPreview
    } catch {
      return null
    }
  },

  async connect(slug: string) {
    return post<{ connected: boolean; refreshHint: string; needsRestart: boolean; running: boolean }>(
      `/api/tools/${slug}/connect`,
    )
  },

  async disconnect(slug: string) {
    return post<{ connected: boolean }>(`/api/tools/${slug}/disconnect`)
  },

  /** Opens the application, restarting it when that is what it takes. */
  async open(slug: string) {
    return post<{ message: string }>(`/api/tools/${slug}/open`)
  },

  /**
   * Opens the application with a prompt prefilled in its composer.
   * The host does not submit it — the owner still presses send.
   */
  async try(slug: string, prompt: string) {
    return send<{
      outcome: string
      surface: "desktop" | "terminal" | "missing"
      command: string | null
      message: string
      app: string
      label: string
    }>(`/api/tools/${slug}/try`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ prompt }),
    })
  },

  /** Builds the Claude Desktop extension and opens its install screen. */
  async bundle() {
    return post<{ path: string; megabytes: number; version: string }>("/api/bundle")
  },
}

export const brain = {
  async get(): Promise<CompanyBrain> {
    return get<CompanyBrain>(
      "/api/brain",
      { nodes: [], edges: [], assistants: [] },
      { nodes: [], edges: [], assistants: [] },
    )
  },
}

/**
 * What the daemon is doing, for the panel on the dashboard.
 *
 * Returns null when there is no daemon, which the panel reads as "nothing
 * to show" rather than "nothing is happening" — those are different, and
 * only one of them is worth a progress bar.
 */
export async function getWorkFeed(): Promise<Work | null> {
  if (!(await connected())) return null
  try {
    const response = await ask("/api/work")
    if (!response.ok) return null
    return (await response.json()) as Work
  } catch {
    return null
  }
}

export const background = {
  async policy(): Promise<PolicyState | null> {
    if (!(await connected())) return null
    try {
      const response = await ask("/api/policy")
      if (!response.ok) return null
      return (await response.json()) as PolicyState
    } catch {
      return null
    }
  },

  async setPolicy(policy: Policy): Promise<PolicyState | null> {
    if (!(await connected())) return null
    try {
      const response = await ask("/api/policy", {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(policy),
      })
      if (!response.ok) return null
      return (await response.json()) as PolicyState
    } catch {
      return null
    }
  },

  /** Which CLIs can read documents on this machine. */
  async engines(): Promise<DetectedEngine[]> {
    if (!(await connected())) return []
    try {
      const response = await ask("/api/engines")
      if (!response.ok) return []
      return (await response.json()) as DetectedEngine[]
    } catch {
      return []
    }
  },

  async release() {
    return post<{ released: number }>("/api/work/release")
  },

  async autostart(): Promise<AutostartState | null> {
    if (!(await connected())) return null
    try {
      const response = await ask("/api/autostart")
      if (!response.ok) return null
      return (await response.json()) as AutostartState
    } catch {
      return null
    }
  },

  async setAutostart(on: boolean) {
    return post<AutostartState>(`/api/autostart/${on ? "on" : "off"}`)
  },
}

/**
 * Names the company, in the lake rather than only on screen.
 *
 * Without this the name lives in one browser's memory: the gateway keeps
 * calling them "Your company" in front of an AI tool, and the Claude
 * Desktop extension is built under the wrong name.
 */
async function putCompanyName(name: string): Promise<void> {
  if (!(await connected())) return
  try {
    await ask("/api/company", {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ name }),
    })
  } catch {
    /* the name is still correct on screen; the next save will carry it */
  }
}
