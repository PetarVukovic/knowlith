import { useEffect, useRef } from "react"
import ForceGraph3D, {
  type ConfigOptions,
  type ForceGraph3DInstance,
  type LinkObject,
  type NodeObject,
} from "3d-force-graph"
import * as THREE from "three"
import SpriteText from "three-spritetext"
import { brainKindMatches, brainVisibleNodeIds } from "@/lib/brainGraph"
import type { BrainEdge, BrainNode } from "@/lib/types"

type GNode = NodeObject & BrainNode
type GLink = LinkObject<GNode> & { type: string; label: string }
type Graph = ForceGraph3DInstance<GNode, GLink>
type GraphCtor = new (el: HTMLElement, cfg?: ConfigOptions) => Graph

const KIND_COLOR: Record<string, string> = {
  rule: "#3b82f6",
  process: "#10b981",
  skill: "#f59e0b",
  term: "#8b5cf6",
  fact: "#8b5cf6",
  document: "#8b979c",
}
const ACCENT = "#0e6e6e"
const CONFLICT = "#e3776d"

const KIND_RADIUS: Record<string, number> = {
  skill: 7,
  process: 6.5,
  rule: 5.5,
  term: 5,
  fact: 5,
  document: 3,
}

const KIND_ANCHOR: Record<string, [number, number, number]> = {
  rule: [-60, 20, 0],
  term: [-30, -10, 40],
  fact: [-30, -10, 40],
  process: [60, 20, 0],
  skill: [40, 50, -30],
  document: [0, -60, 0],
}

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

export function BrainGraph3D({
  nodes,
  edges,
  selectedId,
  kindFilter,
  resetSignal = 0,
  onSelect,
  onOpen,
}: {
  nodes: BrainNode[]
  edges: BrainEdge[]
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
  const edgesRef = useRef(edges)
  edgesRef.current = edges
  const state = useRef({ selectedId, kindFilter, onSelect, onOpen })
  state.current = { selectedId, kindFilter, onSelect, onOpen }

  const linkTouches = (l: GLink, id: string) => {
    const s = (l.source as GNode).id
    const d = (l.target as GNode).id
    return s === id || d === id
  }

  const applyLinkStyle = () => {
    const g = graph.current
    if (!g) return
    const { selectedId } = state.current
    const focus = hovered.current ?? selectedId
    const dark = document.documentElement.classList.contains("dark")

    g.linkColor((l) => {
      if (l.type === "conflicts_with") {
        if (focus && linkTouches(l, focus)) return CONFLICT
        return focus ? `${CONFLICT}55` : CONFLICT
      }
      if (focus) {
        if (linkTouches(l, focus)) {
          return l.type === "quoted_in" ? (dark ? "#6a9a9a" : ACCENT) : ACCENT
        }
        return dark ? "#243135" : "#dde4e6"
      }
      return l.type === "quoted_in" ? (dark ? "#3a4a4f" : "#cfd8db") : dark ? "#5c6f75" : "#94a3b8"
    })
      .linkWidth((l) => {
        if (focus && linkTouches(l, focus)) return l.type === "quoted_in" ? 1.2 : 2.2
        return l.type === "quoted_in" ? 0.35 : 0.8
      })
      .linkOpacity(focus ? 0.85 : 0.45)
      .linkDirectionalParticles((l) => {
        if (!focus || !linkTouches(l, focus)) return 0
        return l.type === "quoted_in" ? 2 : 4
      })
      .linkDirectionalParticleWidth(1.8)
      .linkDirectionalParticleColor(() => ACCENT)
  }

  const restyle = () => {
    const g = graph.current
    if (!g) return
    const { selectedId, kindFilter } = state.current
    const focus = hovered.current ?? selectedId
    const neighbours = new Set<string>()
    if (focus) {
      for (const e of edgesRef.current) {
        if (e.from === focus) neighbours.add(e.to)
        if (e.to === focus) neighbours.add(e.from)
      }
    }
    const matches = (n: BrainNode) => brainKindMatches(kindFilter, n.kind)
    const small = nodes.filter((n) => n.kind !== "document").length <= 80

    for (const n of nodes) {
      const b = built.current.get(n.id)
      if (!b) continue
      const isFocus = n.id === focus
      const near = neighbours.has(n.id)
      let opacity = 1
      if (!matches(n)) opacity = 0.12
      else if (focus && !isFocus && !near) opacity = 0.3
      b.mesh.material.color.set(KIND_COLOR[n.kind] ?? KIND_COLOR.document)
      b.mesh.material.emissive.set(isFocus ? "#ffffff" : near ? "#888888" : "#000000")
      b.mesh.material.emissiveIntensity = isFocus ? 0.35 : near ? 0.12 : 0
      b.mesh.material.opacity = opacity
      const scale = isFocus ? 1.35 : near ? 1.15 : 1
      b.mesh.scale.setScalar(scale)
      b.label.visible = isFocus || near || (small && matches(n) && !focus && n.kind !== "document")
      b.label.material.opacity = opacity
    }

    applyLinkStyle()
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
      .linkDirectionalArrowLength((l) => (l.type === "quoted_in" ? 0 : 2.5))
      .linkDirectionalArrowRelPos(1)
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
      .enableNodeDrag(false)
      .warmupTicks(40)
      .cooldownTicks(140)

    g.d3Force("charge")?.strength(-110)
    g.d3Force("link")?.distance((l: GLink) => (l.type === "quoted_in" ? 30 : 60))
    g.d3Force("kind", (alpha: number) => {
      for (const n of cache.current.values()) {
        const a = KIND_ANCHOR[n.kind] ?? [0, 0, 0]
        n.vx = (n.vx ?? 0) + (a[0] - (n.x ?? 0)) * alpha * 0.03
        n.vy = (n.vy ?? 0) + (a[1] - (n.y ?? 0)) * alpha * 0.03
        n.vz = (n.vz ?? 0) + (a[2] - (n.z ?? 0)) * alpha * 0.03
      }
    })

    applyLinkStyle()

    const ro = new ResizeObserver(() => {
      g.width(el.clientWidth).height(el.clientHeight)
    })
    ro.observe(el)

    const mo = new MutationObserver(() => {
      t = tone()
      g.backgroundColor(t.bg)
      for (const b of built.current.values()) {
        b.label.color = t.ink
        b.label.backgroundColor = t.dark ? "rgba(13,19,21,0.72)" : "rgba(255,255,255,0.82)"
      }
      applyLinkStyle()
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
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

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
    for (const id of [...built.current.keys()]) {
      if (!next.has(id)) built.current.delete(id)
    }
    const visible = brainVisibleNodeIds(nodes, edges, kindFilter)
    const nodeList =
      visible == null ? [...next.values()] : [...next.values()].filter((n) => visible.has(n.id))
    const links: GLink[] = edges
      .filter((e) => {
        if (!next.has(e.from) || !next.has(e.to)) return false
        if (visible == null) return true
        return visible.has(e.from) && visible.has(e.to)
      })
      .map((e) => ({ source: e.from, target: e.to, type: e.type, label: e.label }))
    const refit = !same || !fitted.current
    g.onEngineStop(() => {
      if (!refit) return
      g.zoomToFit(600, 24)
      fitted.current = true
      g.onEngineStop(() => {})
    })
    g.graphData({ nodes: nodeList, links })
    const timers = [window.setTimeout(restyle, 120), window.setTimeout(restyle, 600)]
    return () => timers.forEach((id) => window.clearTimeout(id))
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [nodes, edges, kindFilter])

  useEffect(() => {
    restyle()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedId, kindFilter])

  useEffect(() => {
    if (resetSignal === 0) return
    graph.current?.zoomToFit(600, 40)
  }, [resetSignal])

  return <div ref={host} className="h-full w-full" aria-label="Interactive company knowledge graph" role="img" />
}
