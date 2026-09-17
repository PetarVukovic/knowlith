import type { BrainNode } from "@/lib/types"

/** World size of the cortex mesh. Camera framing is derived from this. */
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
  return (h >>> 0) / 4294967296
}

/**
 * One cerebral hemisphere from a unit-sphere vertex.
 *
 * +Z is anterior, +Y dorsal, +X right. The two lobes are shifted off the
 * midline so a fissure reads; a temporal drop and a flattened belly stop
 * the silhouette looking like a sphere. Kind patches are a scanability
 * device — not a claim that a rule lives in a frontal lobe.
 */
export function hemispherePoint(side: 1 | -1, x: number, y: number, z: number, depth = 1): Vec3 {
  const r = Math.hypot(x, y, z) || 1
  x /= r
  y /= r
  z /= r

  let px = x * 0.68
  let py = y * 0.62
  let pz = z * 0.94

  const lateral = Math.abs(x)
  const ventral = Math.max(0, -y)
  py -= ventral * lateral * 0.28
  px += Math.sign(x || side) * ventral * lateral * 0.12
  pz += ventral * lateral * 0.08

  if (z > 0.22) pz += 0.06 * z
  if (z < -0.18) pz -= 0.04
  if (py < -0.28) py = -0.28 + (py + 0.28) * 0.5

  const split = 0.125 + 0.05 * ventral
  const inner = x * side < 0
  px += side * split
  if (inner) {
    px = side * (0.035 + 0.02 * Math.abs(y))
  }

  if (!inner) {
    const g =
      0.032 * Math.sin(pz * 17 + py * 4) * (0.45 + 0.55 * lateral) +
      0.018 * Math.sin(py * 15 + pz * 7) +
      0.012 * Math.sin(pz * 28) * lateral
    const nlen = Math.hypot(px, py, pz) || 1
    px += (px / nlen) * g
    py += (py / nlen) * g
    pz += (pz / nlen) * g
  }

  const s = BRAIN_SCALE * depth
  return { x: px * s, y: py * s, z: pz * s }
}

/** Smaller posterior-ventral pair. Visual mass only — no nodes sit here. */
export function cerebellumPoint(x: number, y: number, z: number): Vec3 {
  const r = Math.hypot(x, y, z) || 1
  x /= r
  y /= r
  z /= r
  let px = x * 0.3
  let py = y * 0.22 - 0.4
  let pz = z * 0.26 - 0.58
  if (Math.abs(px) < 0.04) py -= 0.02
  const s = BRAIN_SCALE
  return { x: px * s, y: py * s, z: pz * s }
}

type Patch = { side: 1 | -1 | 0; theta: [number, number]; phi: [number, number]; depth: number }

const KIND_PATCH: Record<string, Patch> = {
  rule: { side: -1, theta: [0.45, 1.25], phi: [-0.85, 0.85], depth: 1.04 },
  process: { side: 1, theta: [0.45, 1.25], phi: [-0.85, 0.85], depth: 1.04 },
  skill: { side: 0, theta: [0.18, 0.7], phi: [-2.2, 2.2], depth: 1.05 },
  term: { side: 0, theta: [0.5, 1.4], phi: [-2.05, 2.05], depth: 1.03 },
  fact: { side: 0, theta: [0.5, 1.4], phi: [-2.05, 2.05], depth: 1.03 },
  document: { side: 0, theta: [0.5, 1.6], phi: [-2.4, 2.4], depth: 0.72 },
}

function placeBrainNode(id: string, kind: string, index: number, ofKind: number): Vec3 {
  const patch = KIND_PATCH[kind] ?? KIND_PATCH.term
  const j = unitHash(id, 1)
  const j2 = unitHash(id, 2)
  const t = ofKind <= 1 ? 0.5 : (index + 0.5) / ofKind
  const theta =
    patch.theta[0] + (patch.theta[1] - patch.theta[0]) * Math.min(0.97, Math.max(0.03, t * 0.82 + j * 0.18))
  const span = patch.phi[1] - patch.phi[0]
  const phi = patch.phi[0] + span * ((index * 0.6180339887 + j2) % 1)
  const side: 1 | -1 = patch.side === 0 ? (index % 2 === 0 ? -1 : 1) : patch.side
  const st = Math.sin(theta)
  const x = st * Math.abs(Math.sin(phi)) * side
  const y = Math.cos(theta)
  const z = st * Math.cos(phi)
  const depth = patch.depth + (j - 0.5) * 0.03
  return hemispherePoint(side, x, y, z, depth)
}

/**
 * Stable cortex coordinates for every node. Objects sit on the surface in
 * kind patches; a document sits inward of the objects that quote it, so the
 * owner sees evidence under the claim rather than a second cloud.
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
    list.forEach((n, i) => pos.set(n.id, placeBrainNode(n.id, n.kind, i, list.length)))
  }

  const docs = byKind.get("document") ?? []
  docs.forEach((doc, i) => {
    const parentPos: Vec3[] = []
    for (const e of edges) {
      const other = e.to === doc.id ? e.from : e.from === doc.id ? e.to : null
      if (other == null) continue
      const p = pos.get(other)
      if (p) parentPos.push(p)
    }
    if (parentPos.length > 0) {
      const avg = {
        x: parentPos.reduce((s, p) => s + p.x, 0) / parentPos.length,
        y: parentPos.reduce((s, p) => s + p.y, 0) / parentPos.length,
        z: parentPos.reduce((s, p) => s + p.z, 0) / parentPos.length,
      }
      pos.set(doc.id, { x: avg.x * 0.64, y: avg.y * 0.64, z: avg.z * 0.64 })
    } else {
      pos.set(doc.id, placeBrainNode(doc.id, "document", i, docs.length))
    }
  })
  return pos
}

export function brainCameraHome(): { position: Vec3; lookAt: Vec3 } {
  const dist = BRAIN_SCALE * 2.85
  return {
    position: { x: dist * 0.62, y: dist * 0.22, z: dist * 0.58 },
    lookAt: { x: 0, y: -BRAIN_SCALE * 0.08, z: -BRAIN_SCALE * 0.06 },
  }
}
