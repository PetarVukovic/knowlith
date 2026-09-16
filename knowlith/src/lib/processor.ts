import type { Processor } from "./types"
import type { UiMode } from "@/state/AppState"

/**
 * What the owner is told about where their documents are read.
 *
 * Simple mode names whose account is reading, not which binary. This used to
 * say "Reads on this Mac", and it did not: the CLI runs here, the text goes
 * to the vendor. A status bar is not the place to explain that, but it must
 * not contradict the onboarding screen that does.
 *
 * Engineer mode names the engine, because there the question actually being
 * asked is "which one produced this run", and that has an exact answer.
 */
const ENGINEER: Record<Processor, string> = {
  codex: "Codex CLI on this Mac",
  "claude-code": "Claude Code CLI on this Mac",
  "cursor-agent": "Cursor Agent CLI on this Mac",
  managed: "Knowlith Managed",
}

const SIMPLE: Record<Processor, string> = {
  codex: "Read by your Codex account",
  "claude-code": "Read by your Claude account",
  "cursor-agent": "Read by your Cursor account",
  managed: "Read in Knowlith's cloud",
}

export function processorLabel(processor: Processor, mode: UiMode): string {
  return mode === "engineer" ? ENGINEER[processor] : SIMPLE[processor]
}
