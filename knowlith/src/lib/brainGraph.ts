import type { BrainNode } from "@/lib/types"

/** Whether a graph node belongs to the sidebar category filter. */
export function brainKindMatches(kindFilter: string, kind: string): boolean {
  return (
    kindFilter === "all" ||
    kind === kindFilter ||
    (kindFilter === "fact" && (kind === "term" || kind === "fact"))
  )
}

/** Lake document id from a brain document node (handles legacy double prefix). */
export function brainDocumentLakeId(nodeId: string): string {
  if (nodeId.startsWith("doc:doc:")) return nodeId.slice(4)
  return nodeId.startsWith("doc:") ? nodeId : `doc:${nodeId}`
}

/** Node ids to draw when a category filter is active — matching objects plus quoted files. */
export function brainVisibleNodeIds(
  nodes: BrainNode[],
  edges: { from: string; to: string }[],
  kindFilter: string,
): Set<string> | null {
  if (kindFilter === "all") return null
  const keep = new Set<string>()
  for (const n of nodes) {
    if (brainKindMatches(kindFilter, n.kind)) keep.add(n.id)
  }
  for (const e of edges) {
    if (keep.has(e.from) && e.to.startsWith("doc:")) keep.add(e.to)
    if (keep.has(e.to) && e.from.startsWith("doc:")) keep.add(e.from)
  }
  return keep
}
