import type { BrainNode } from "@/lib/types"

/** Default span of the knowledge graph. */
export const BRAIN_SCALE = 72

export type Vec3 = { x: number; y: number; z: number }

/**
 * Kind colours sit next to the product tokens (info / confirmed / pending)
 * rather than the default Tailwind rainbow the first graph shipped with.
 * Documents stay faint: they are evidence, not claims.
 */
export const BRAIN_KIND_COLOR: Record<string, string> = {
  rule: "#3d7eb5",
  process: "#2a8f72",
  skill: "#c4892e",
  term: "#6d73b0",
  fact: "#6d73b0",
  document: "#8b979c",
}

export const BRAIN_LEGEND = [
  ["all", "All", "#8b979c"],
  ["rule", "Rules", BRAIN_KIND_COLOR.rule],
  ["process", "Processes", BRAIN_KIND_COLOR.process],
  ["skill", "Skills", BRAIN_KIND_COLOR.skill],
  ["fact", "Terms", BRAIN_KIND_COLOR.fact],
  ["document", "Documents", BRAIN_KIND_COLOR.document],
] as const

export const BRAIN_KIND_RADIUS: Record<string, number> = {
  skill: 5.1,
  process: 4.7,
  rule: 4.3,
  term: 3.9,
  fact: 3.9,
  document: 2.3,
}

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

/** FNV-1a folded into [0, 1). Same id always lands in the same place. */
export function unitHash(id: string, salt = 0): number {
  let h = 2166136261 ^ salt
  for (let i = 0; i < id.length; i++) {
    h ^= id.charCodeAt(i)
    h = Math.imul(h, 16777619)
  }
  h = Math.imul(h ^ (h >>> 16), 0x85ebca6b)
  h = Math.imul(h ^ (h >>> 13), 0xc2b2ae35)
  return ((h ^ (h >>> 16)) >>> 0) / 4294967296
}

const KIND_ANCHOR: Record<string, Vec3> = {
  rule: { x: -48, y: 26, z: 12 },
  process: { x: 48, y: 15, z: -12 },
  skill: { x: 20, y: 52, z: 30 },
  term: { x: -12, y: -38, z: 20 },
  fact: { x: -12, y: -38, z: 20 },
  document: { x: 0, y: -8, z: -35 },
}

function placeBrainNode(id: string, kind: string): Vec3 {
  const anchor = KIND_ANCHOR[kind] ?? KIND_ANCHOR.fact
  return {
    x: anchor.x + (unitHash(id, 1) - 0.5) * 72,
    y: anchor.y + (unitHash(id, 2) - 0.5) * 72,
    z: anchor.z + (unitHash(id, 3) - 0.5) * 90,
  }
}

/**
 * Stable, spatial kind clusters. Evidence sits near the claims quoting it.
 * Coordinates depend on identity, not the number or order of discoveries.
 */
export function layoutBrainNodes(
  nodes: BrainNode[],
  edges: { from: string; to: string }[],
): Map<string, Vec3> {
  const byKind = new Map<string, BrainNode[]>()
  for (const n of nodes) {
    const key = n.kind === "term" ? "fact" : n.kind
    const list = byKind.get(key) ?? []
    list.push(n)
    byKind.set(key, list)
  }
  for (const list of byKind.values()) list.sort((a, b) => a.id.localeCompare(b.id))

  const pos = new Map<string, Vec3>()
  for (const [kind, list] of byKind) {
    if (kind === "document") continue
    list.forEach((n) => pos.set(n.id, placeBrainNode(n.id, n.kind)))
  }

  const docs = byKind.get("document") ?? []
  const neighbours = new Map<string, Vec3[]>()
  for (const e of edges) {
    for (const [id, other] of [[e.from, e.to], [e.to, e.from]]) {
      const p = pos.get(other)
      if (p) {
        const list = neighbours.get(id) ?? []
        list.push(p)
        neighbours.set(id, list)
      }
    }
  }
  docs.forEach((doc) => {
    const parentPos = neighbours.get(doc.id) ?? []
    if (parentPos.length > 0) {
      const avg = {
        x: parentPos.reduce((s, p) => s + p.x, 0) / parentPos.length,
        y: parentPos.reduce((s, p) => s + p.y, 0) / parentPos.length,
        z: parentPos.reduce((s, p) => s + p.z, 0) / parentPos.length,
      }
      pos.set(doc.id, { x: avg.x * 0.64, y: avg.y * 0.64, z: avg.z * 0.64 })
    } else {
      pos.set(doc.id, placeBrainNode(doc.id, "document"))
    }
  })
  return pos
}

export function brainCameraHome(): { position: Vec3; lookAt: Vec3 } {
  const dist = BRAIN_SCALE * 2.85
  return {
    position: { x: dist * 0.12, y: dist * 0.08, z: dist * 1.25 },
    lookAt: { x: 0, y: 0, z: 0 },
  }
}
