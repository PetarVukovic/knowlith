/**
 * The only module that knows where data comes from.
 *
 * It asks the daemon on `127.0.0.1` first, and falls back to the demo
 * fixtures when nothing answers. That fallback is not a convenience for
 * development — it is what lets somebody open the interface, see what the
 * product does, and only then install anything. Once `knowlith serve` is
 * running, the same screens show their own company instead.
 */
import * as fixtures from "./mock"
import { sleep } from "./utils"
import type {
  CompilerRun,
  SourceDocument,
  ToolRead,
  ContextObject,
  DiscoverySummary,
  MergeHint,
  ReviewItem,
  SkillDoc,
  Source,
  TreeNode,
} from "./types"

const LATENCY = 120

/** Where the daemon listens. Overridable for a non-default port. */
const DAEMON =
  (import.meta.env.VITE_KNOWLITH_API as string | undefined)?.replace(/\/$/, "") ?? "http://127.0.0.1:7717"

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
      const response = await fetch(`${DAEMON}/api/health`, { signal: controller.signal })
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
 * One request, with the fixtures as the answer when there is no daemon.
 *
 * A daemon that is reachable but fails a particular endpoint falls back too:
 * an empty screen tells the owner nothing, and a thrown error in a data
 * layer takes the whole page with it.
 */
async function get<T>(path: string, fallback: T): Promise<T> {
  if (!(await connected())) {
    await sleep(LATENCY)
    return fallback
  }
  try {
    const response = await fetch(`${DAEMON}${path}`)
    if (!response.ok) return fallback
    return (await response.json()) as T
  } catch {
    return fallback
  }
}

async function post<T>(path: string, body?: unknown): Promise<T | null> {
  if (!(await connected())) return null
  try {
    const response = await fetch(`${DAEMON}${path}`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body ?? {}),
    })
    if (!response.ok) return null
    return (await response.json()) as T
  } catch {
    return null
  }
}

export const api = {
  async getCompany() {
    return get<{ name: string }>("/api/company", fixtures.company)
  },
  async getTree(): Promise<TreeNode[]> {
    return get<TreeNode[]>("/api/tree", fixtures.contextTree)
  },
  async getObjects(): Promise<ContextObject[]> {
    return get<ContextObject[]>("/api/objects", fixtures.contextObjects)
  },
  async getReviewQueue(): Promise<ReviewItem[]> {
    return get<ReviewItem[]>("/api/review", fixtures.reviewQueue)
  },
  async getSources(): Promise<Source[]> {
    return get<Source[]>("/api/sources", fixtures.sources)
  },
  async getSkills(): Promise<SkillDoc[]> {
    return get<SkillDoc[]>("/api/skills", fixtures.skills)
  },
  async getDiscovery(): Promise<DiscoverySummary> {
    return get<DiscoverySummary>("/api/discovery", fixtures.discovery)
  },
  async getCompilerRuns(): Promise<CompilerRun[]> {
    return get<CompilerRun[]>("/api/runs", fixtures.compilerRuns)
  },
  async getSourceDocuments(): Promise<SourceDocument[]> {
    return get<SourceDocument[]>("/api/documents", fixtures.sourceDocuments)
  },
  /** Reads the MCP gateway actually recorded, keyed by object id. */
  async getToolReads(): Promise<Record<string, ToolRead[]>> {
    return get<Record<string, ToolRead[]>>("/api/tool-reads", fixtures.toolReads)
  },
  async getRecentActivity() {
    return get<typeof fixtures.recentActivity>("/api/activity", fixtures.recentActivity)
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
   * Pairs the daemon thinks may be one rule written twice.
   *
   * There is no fixture fallback with content here on purpose. A demo that
   * invents a duplicate teaches the owner to expect a question the daemon
   * may never ask about their own folder.
   */
  async getMergeHints(): Promise<MergeHint[]> {
    return get<MergeHint[]>("/api/merge-hints", [])
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
  async getWork(): Promise<{ queued: number; working: number }> {
    const health = await get<{ queued?: number; working?: number }>("/api/health", {})
    return { queued: health.queued ?? 0, working: health.working ?? 0 }
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
