import { useEffect, useRef } from "react"
import ForceGraph3D, {
  type ConfigOptions,
  type ForceGraph3DInstance,
  type LinkObject,
  type NodeObject,
} from "3d-force-graph"
import * as THREE from "three"
import SpriteText from "three-spritetext"
import type { BrainEdge, BrainNode } from "@/lib/types"

type GNode = NodeObject & BrainNode
type GLink = LinkObject<GNode> & { type: string; label: string }
// The package types the generics on the instance but not on the exported
// constructor, so the one `new` goes through this signature.
type Graph = ForceGraph3DInstance<GNode, GLink>
type GraphCtor = new (el: HTMLElement, cfg?: ConfigOptions) => Graph

/** Kind → colour. Documents are deliberately grey: they are where knowledge came from, not knowledge. */
const KIND_COLOR: Record<string, string> = {
  rule: "#3b82f6",
  process: "#10b981",
  skill: "#f59e0b",
  term: "#8b5cf6",
  fact: "#8b5cf6",
  document: "#8b979c",
}
const LIT = "#f59e0b"
const CONFLICT = "#e3776d"

const KIND_RADIUS: Record<string, number> = {
  skill: 7,
  process: 6.5,
  rule: 5.5,
  term: 5,
  fact: 5,
  document: 3,
}

/**
 * A stable home for each kind, so rules gather on one side and processes on
 * another however the simulation settles. Documents sit low, under the
 * things that quote them.
 */
const KIND_ANCHOR: Record<string, [number, number, number]> = {
  rule: [-60, 20, 0],
  term: [-30, -10, 40],
  fact: [-30, -10, 40],
  process: [60, 20, 0],
  skill: [40, 50, -30],
  document: [0, -60, 0],
}

/** Same id, same starting point — refresh must not reshuffle the map. */
function seed(id: string): [number, number, number] {
  let h = 2166136261
  for (let i = 0; i < id.length; i++) {
    h ^= id.charCodeAt(i)
    h = Math.imul(h, 16777619)
  }
  const u = h >>> 0
  return [((u % 1000) / 500 - 1) * 60, (((u >>> 10) % 1000) / 500 - 1) * 60, (((u >>> 20) % 1000) / 500 - 1) * 60]
}

type Built = { mesh: THREE.Mesh<THREE.SphereGeometry, THREE.MeshLambertMaterial>; label: SpriteText }

/**
 * The company brain in three dimensions.
 *
 * Vanilla three.js under a ref, not a React wrapper: the graph is an
 * external system, and rendering it through React state re-created the
 * scene on every poll. Selection, hover, filter and lit reads mutate the
 * materials in place; only a change in *which* nodes exist rebuilds.
 */
export function BrainGraph3D({
  nodes,
  edges,
  litIds,
  selectedId,
  kindFilter,
  resetSignal = 0,
  onSelect,
  onOpen,
}: {
  nodes: BrainNode[]
  edges: BrainEdge[]
  litIds: string[]
  selectedId: string | null
  kindFilter: string
  resetSignal?: number
  onSelect: (node: BrainNode | null) => void
  onOpen: (node: BrainNode) => void
}) {
  const host = useRef<HTMLDivElement>(null)
  const graph = useRef<Graph | null>(null)
  const cache = useRef(new Map<string, GNode>())
  const built = useRef(new Map<string, Built>())
  const hovered = useRef<string | null>(null)
  const fitted = useRef(false)
  const state = useRef({ litIds, selectedId, kindFilter, onSelect, onOpen })
  state.current = { litIds, selectedId, kindFilter, onSelect, onOpen }

  const restyle = () => {
    const g = graph.current
    if (!g) return
    const { litIds, selectedId, kindFilter } = state.current
    const lit = new Set(litIds)
    const litOn = lit.size > 0
    const hover = hovered.current
    const neighbours = new Set<string>()
    const focus = hover ?? selectedId
    if (focus) {
      for (const e of edges) {
        if (e.from === focus) neighbours.add(e.to)
        if (e.to === focus) neighbours.add(e.from)
      }
    }
    const matches = (n: BrainNode) =>
      kindFilter === "all" ||
      n.kind === kindFilter ||
      (kindFilter === "fact" && (n.kind === "term" || n.kind === "fact"))
    // Documents never carry a resting label, so they do not count toward
    // the point where a labelled graph turns into an unreadable one.
    const small = nodes.filter((n) => n.kind !== "document").length <= 80

    for (const n of nodes) {
      const b = built.current.get(n.id)
      if (!b) continue
      const isLit = lit.has(n.id)
      const isFocus = n.id === focus
      const near = neighbours.has(n.id)
      let opacity = 1
      if (!matches(n)) opacity = 0.12
      else if (litOn && !isLit) opacity = 0.25
      else if (focus && !isFocus && !near) opacity = 0.35
      b.mesh.material.color.set(isLit ? LIT : KIND_COLOR[n.kind] ?? KIND_COLOR.document)
      b.mesh.material.emissive.set(isLit ? LIT : isFocus ? "#ffffff" : "#000000")
      b.mesh.material.emissiveIntensity = isLit ? 0.9 : isFocus ? 0.25 : 0
      b.mesh.material.opacity = opacity
      const scale = isLit ? 1.5 : isFocus ? 1.3 : near ? 1.1 : 1
      b.mesh.scale.setScalar(scale)
      b.label.visible =
        isLit || isFocus || near || (small && matches(n) && !litOn && n.kind !== "document")
      b.label.material.opacity = opacity
    }

    g.linkColor(g.linkColor())
      .linkWidth(g.linkWidth())
      .linkOpacity(litOn ? 0.18 : 0.45)
      .linkDirectionalParticles(g.linkDirectionalParticles())
  }

  useEffect(() => {
    const el = host.current
    if (!el) return
    const builtMap = built.current
    const cacheMap = cache.current

    const g = new (ForceGraph3D as unknown as GraphCtor)(el, { controlType: "orbit" })
    graph.current = g

    const tone = () => {
      const css = getComputedStyle(document.documentElement)
      return {
        bg: css.getPropertyValue("--k-surface").trim() || "#ffffff",
        ink: css.getPropertyValue("--k-text").trim() || "#0f1619",
        dark: document.documentElement.classList.contains("dark"),
      }
    }
    let t = tone()

    g.backgroundColor(t.bg)
      .showNavInfo(false)
      .width(el.clientWidth)
      .height(el.clientHeight)
      .nodeThreeObject((n) => {
        const r = KIND_RADIUS[n.kind] ?? 3
        const mesh = new THREE.Mesh(
          new THREE.SphereGeometry(r, 24, 16),
          new THREE.MeshLambertMaterial({
            color: KIND_COLOR[n.kind] ?? KIND_COLOR.document,
            transparent: true,
            opacity: 1,
          }),
        )
        const label = new SpriteText(n.title.length > 34 ? `${n.title.slice(0, 33)}…` : n.title)
        label.material.depthWrite = false
        label.material.transparent = true
        label.color = t.ink
        label.backgroundColor = t.dark ? "rgba(13,19,21,0.72)" : "rgba(255,255,255,0.82)"
        label.padding = 1.2
        label.borderRadius = 2
        label.textHeight = n.kind === "document" ? 3 : 4
        label.position.y = -(r + 6)
        label.visible = false
        const group = new THREE.Group()
        group.add(mesh)
        group.add(label)
        built.current.set(n.id, { mesh, label })
        return group
      })
      .nodeLabel(() => "")
      .linkLabel((l) => {
        const s = l.source as GNode
        const d = l.target as GNode
        return `${s.title} ${l.label} ${d.title}`
      })
      .linkColor((l) => {
        if (l.type === "conflicts_with") return CONFLICT
        const { litIds } = state.current
        const s = (l.source as GNode).id
        const d = (l.target as GNode).id
        if (litIds.includes(s) && litIds.includes(d)) return LIT
        return l.type === "quoted_in" ? (t.dark ? "#3a4a4f" : "#cfd8db") : t.dark ? "#5c6f75" : "#94a3b8"
      })
      .linkWidth((l) => {
        const { litIds } = state.current
        const s = (l.source as GNode).id
        const d = (l.target as GNode).id
        if (litIds.includes(s) && litIds.includes(d)) return 1.6
        return l.type === "quoted_in" ? 0.35 : 0.8
      })
      .linkDirectionalArrowLength((l) => (l.type === "quoted_in" ? 0 : 2.5))
      .linkDirectionalArrowRelPos(1)
      .linkDirectionalParticles((l) => {
        const { litIds } = state.current
        const s = (l.source as GNode).id
        const d = (l.target as GNode).id
        return litIds.includes(s) && litIds.includes(d) ? 3 : 0
      })
      .linkDirectionalParticleWidth(1.6)
      .linkDirectionalParticleColor(() => LIT)
      .onNodeHover((n) => {
        hovered.current = n ? String(n.id) : null
        el.style.cursor = n ? "pointer" : ""
        restyle()
      })
      .onNodeClick((n, ev) => {
        if (ev.detail >= 2 && n.kind !== "document") {
          state.current.onOpen(n)
          return
        }
        state.current.onSelect(n)
        const dist = 90
        const ratio = 1 + dist / Math.hypot(n.x ?? 1, n.y ?? 1, n.z ?? 1)
        g.cameraPosition(
          { x: (n.x ?? 0) * ratio, y: (n.y ?? 0) * ratio, z: (n.z ?? 0) * ratio },
          { x: n.x ?? 0, y: n.y ?? 0, z: n.z ?? 0 },
          700,
        )
      })
      .onBackgroundClick(() => state.current.onSelect(null))
      // Turn and zoom, not rearrange: a node dragged in 3D lands somewhere
      // the next refresh cannot reproduce. Drag-end also makes the library
      // fire a pointerup with no pointer id, which three's OrbitControls
      // (r186) rejects with an uncaught error.
      .enableNodeDrag(false)
      .warmupTicks(40)
      .cooldownTicks(140)

    g.d3Force("charge")?.strength(-110)
    g.d3Force("link")?.distance((l: GLink) => (l.type === "quoted_in" ? 30 : 60))
    // A gentle pull toward the kind's home. Weak enough that links still
    // decide the shape; strong enough that rules end up together.
    g.d3Force("kind", (alpha: number) => {
      for (const n of cache.current.values()) {
        const a = KIND_ANCHOR[n.kind] ?? [0, 0, 0]
        n.vx = (n.vx ?? 0) + (a[0] - (n.x ?? 0)) * alpha * 0.03
        n.vy = (n.vy ?? 0) + (a[1] - (n.y ?? 0)) * alpha * 0.03
        n.vz = (n.vz ?? 0) + (a[2] - (n.z ?? 0)) * alpha * 0.03
      }
    })

    const ro = new ResizeObserver(() => {
      g.width(el.clientWidth).height(el.clientHeight)
    })
    ro.observe(el)

    // Theme flips under the canvas; the background must follow.
    const mo = new MutationObserver(() => {
      t = tone()
      g.backgroundColor(t.bg)
      for (const b of built.current.values()) {
        b.label.color = t.ink
        b.label.backgroundColor = t.dark ? "rgba(13,19,21,0.72)" : "rgba(255,255,255,0.82)"
      }
      g.linkColor(g.linkColor())
    })
    mo.observe(document.documentElement, { attributes: true, attributeFilter: ["class"] })

    return () => {
      ro.disconnect()
      mo.disconnect()
      g._destructor()
      graph.current = null
      builtMap.clear()
      cacheMap.clear()
    }
    // Mounted once; everything after is a mutation.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // Data: reuse node objects by id so positions survive a poll.
  useEffect(() => {
    const g = graph.current
    if (!g) return
    const next = new Map<string, GNode>()
    for (const n of nodes) {
      const prev = cache.current.get(n.id)
      if (prev) {
        prev.title = n.title
        prev.kind = n.kind
        prev.status = n.status
        next.set(n.id, prev)
      } else {
        const [x, y, z] = seed(n.id)
        next.set(n.id, { ...n, x, y, z })
      }
    }
    const same =
      next.size === cache.current.size && [...next.keys()].every((k) => cache.current.has(k))
    cache.current = next
    // Prune, never clear: three reuses the objects of nodes that are still
    // here and only builds new ones, so a cleared map would stay empty and
    // every later restyle would find nothing to touch.
    for (const id of [...built.current.keys()]) {
      if (!next.has(id)) built.current.delete(id)
    }
    const links: GLink[] = edges
      .filter((e) => next.has(e.from) && next.has(e.to))
      .map((e) => ({ source: e.from, target: e.to, type: e.type, label: e.label }))
    // Fit once the simulation has stopped, not on a timer: the seeded
    // positions are wider than the settled shape, so a fit taken while the
    // layout is still contracting leaves the graph small in the middle.
    const refit = !same || !fitted.current
    g.onEngineStop(() => {
      if (!refit) return
      g.zoomToFit(600, 24)
      fitted.current = true
      g.onEngineStop(() => {})
    })
    g.graphData({ nodes: [...next.values()], links })
    // three builds the node objects on its next frames, not synchronously,
    // so a restyle taken at once would find nothing to style.
    const timers = [window.setTimeout(restyle, 120), window.setTimeout(restyle, 600)]
    return () => timers.forEach((t) => window.clearTimeout(t))
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [nodes, edges])

  useEffect(() => {
    restyle()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [litIds, selectedId, kindFilter])

  // Fly to what the assistant is reading.
  useEffect(() => {
    const g = graph.current
    if (!g || litIds.length === 0) return
    const pts = litIds.map((id) => cache.current.get(id)).filter((n): n is GNode => Boolean(n))
    if (pts.length === 0) return
    const c = pts.reduce(
      (acc, n) => ({ x: acc.x + (n.x ?? 0) / pts.length, y: acc.y + (n.y ?? 0) / pts.length, z: acc.z + (n.z ?? 0) / pts.length }),
      { x: 0, y: 0, z: 0 },
    )
    const spread = Math.max(
      40,
      ...pts.map((n) => Math.hypot((n.x ?? 0) - c.x, (n.y ?? 0) - c.y, (n.z ?? 0) - c.z)),
    )
    const dist = spread * 2.2 + 60
    const len = Math.hypot(c.x, c.y, c.z) || 1
    g.cameraPosition({ x: c.x + (c.x / len) * dist, y: c.y + (c.y / len) * dist + 20, z: c.z + (c.z / len) * dist + 40 }, c, 900)
  }, [litIds])

  // The parent's Reset button: bump the counter, the camera fits everything.
  useEffect(() => {
    if (resetSignal === 0) return
    graph.current?.zoomToFit(600, 40)
  }, [resetSignal])

  return <div ref={host} className="h-full w-full" aria-label="Interactive company knowledge graph" role="img" />
}
