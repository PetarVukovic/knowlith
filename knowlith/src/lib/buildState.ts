import type { Work } from "./types"

/** Queue state, never an estimate derived from elapsed time or graph size. */
export function buildState(work: Work | null, discoveries: number) {
  if (!work) return "connecting"
  if (work.stage === "held") return "paused"
  if (work.stage !== "idle") return "working"
  if (work.lines.some((line) => line.state === "failed")) return "attention"
  return discoveries > 0 ? "ready" : "empty"
}
