import type { Processor } from "./types"
import type { UiMode } from "@/state/AppState"

/**
 * What the owner is told about where their documents are read.
 *
 * Simple mode names a place, never a vendor. The owner made one decision, on
 * one onboarding screen, and it was about privacy — "this stays on my Mac" —
 * not about whose model spins behind it. Repeating "Codex worker connected"
 * in the status bar for every second of every session turns that single
 * decision into a permanent reminder that somebody else's agent is grinding
 * through their files, which is the exact impression this product exists to
 * remove.
 *
 * Engineer mode names the engine, because there the question actually being
 * asked is "which one produced this run", and that has an exact answer.
 */
const ENGINEER: Record<Processor, string> = {
  codex: "Codex on this Mac",
  "claude-code": "Claude Code on this Mac",
  "cursor-agent": "Cursor Agent on this Mac",
  managed: "Knowlith Managed",
}

const SIMPLE: Record<Processor, string> = {
  codex: "Reads on this Mac",
  "claude-code": "Reads on this Mac",
  "cursor-agent": "Reads on this Mac",
  managed: "Reads in Knowlith's cloud",
}

export function processorLabel(processor: Processor, mode: UiMode): string {
  return mode === "engineer" ? ENGINEER[processor] : SIMPLE[processor]
}
