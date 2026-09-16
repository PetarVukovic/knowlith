import { failed, tools as toolsApi } from "@/lib/api"
import type { AiTool, ObjectKind } from "@/lib/types"

export type AskOk = {
  ok: true
  label: string
  surface: "desktop" | "terminal" | "missing"
  command: string | null
  message: string
}

export type AskErr = {
  ok: false
  error: string
  needsConnect: boolean
}

/**
 * Prefer a connected assistant that can actually launch on this machine.
 * Terminal CLIs (Cursor Agent, Claude Code, Codex CLI) before desktop when
 * both are connected — those open in Terminal rather than a missing .app.
 */
export function pickAskTarget(tools: AiTool[]): AiTool | null {
  const connected = tools.filter((t) => t.connected && t.launchSurface !== "missing")
  const order: AiTool["slug"][] = ["cursor", "claude-code", "codex", "claude-desktop"]
  for (const slug of order) {
    const hit = connected.find((t) => t.slug === slug)
    if (hit) return hit
  }
  return connected[0] ?? null
}

/** Opens the best connected AI with a prepared question. */
export async function askConnectedAi(prompt: string): Promise<AskOk | AskErr> {
  const listed = await toolsApi.list()
  const target = pickAskTarget(listed)
  if (!target) {
    return { ok: false, error: "Connect an AI assistant first.", needsConnect: true }
  }
  const result = await toolsApi.try(target.slug, prompt)
  if (failed(result)) {
    return { ok: false, error: result.error, needsConnect: false }
  }
  return {
    ok: true,
    label: result.label,
    surface: result.surface,
    command: result.command,
    message: result.message,
  }
}

export function companyKnowledgePrompt(company: string): string {
  return (
    `Using only Knowlith tools, tell me what you know about ${company}. ` +
    `Call get_relevant_context for this company, read what it lists, then call check_coverage. ` +
    `Say clearly what you could verify from approved knowledge and what you could not.`
  )
}

/** Prefill when the owner tries one piece of company knowledge in an AI tool. */
export function objectTryPrompt(kind: ObjectKind, title: string, company: string): string {
  const name = title.trim()
  switch (kind) {
    case "skill":
      return (
        `Use Knowlith's "${name}" skill. Walk me through how ${company} actually does this. ` +
        `Call get_relevant_context and check_coverage before you finish.`
      )
    case "rule":
      return (
        `Using only Knowlith, apply the company rule "${name}" for ${company}. ` +
        `Call get_relevant_context and check_coverage. Cite the document you read.`
      )
    case "process":
      return (
        `Using only Knowlith, walk me through the process "${name}" the way ${company} actually does it. ` +
        `Call get_relevant_context and check_coverage. Keep the steps in order.`
      )
    case "term":
    case "fact":
      return (
        `Using only Knowlith, explain what "${name}" means at ${company}. ` +
        `Call get_relevant_context and check_coverage. Do not invent a definition.`
      )
  }
}

export function tryButtonLabel(kind: ObjectKind): string {
  switch (kind) {
    case "skill":
      return "Try this skill in your AI"
    case "rule":
      return "Try this rule in your AI"
    case "process":
      return "Try this process in your AI"
    case "term":
    case "fact":
      return "Ask AI about this"
  }
}
