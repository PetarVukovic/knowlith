import { useEffect, useRef } from "react"
import ForceGraph3D, {
  type ConfigOptions,
  type ForceGraph3DInstance,
  type LinkObject,
  type NodeObject,
} from "3d-force-graph"
import * as THREE from "three"
import SpriteText from "three-spritetext"
import {
  BRAIN_KIND_COLOR,
  BRAIN_KIND_RADIUS,
  brainCameraHome,
  brainKindMatches,
  brainVisibleNodeIds,
  cerebellumPoint,
  hemispherePoint,
  layoutBrainNodes,
  unitHash,
} from "@/lib/brainGraph"
import type { BrainEdge, BrainNode } from "@/lib/types"

type GNode = NodeObject & BrainNode
type GLink = LinkObject<GNode> & { type: string; label: string }
type Graph = ForceGraph3DInstance<GNode, GLink>
type GraphCtor = new (el: HTMLElement, cfg?: ConfigOptions) => Graph

const ACCENT = "#0e6e6e"
const CONFLICT = "#e3776d"

type Built = {
  core: THREE.Mesh<THREE.SphereGeometry, THREE.MeshPhongMaterial>
  halo: THREE.Mesh<THREE.SphereGeometry, THREE.MeshPhongMaterial>
  label: SpriteText
}

type Cortex = {
  group: THREE.Group
  paint: (dark: boolean) => void
  dispose: () => void
}

function deformSphere(
  geo: THREE.SphereGeometry,
  map: (x: number, y: number, z: number) => { x: number; y: number; z: number },
) {
  const pos = geo.attributes.position
  for (let i = 0; i < pos.count; i++) {
    const p = map(pos.getX(i), pos.getY(i), pos.getZ(i))
    pos.setXYZ(i, p.x, p.y, p.z)
  }
  geo.computeVertexNormals()
}

function buildCortex(): Cortex {
  const leftGeo = new THREE.SphereGeometry(1, 64, 48)
  const rightGeo = new THREE.SphereGeometry(1, 64, 48)
  const cereGeo = new THREE.SphereGeometry(1, 32, 24)
  deformSphere(leftGeo, (x, y, z) => hemispherePoint(-1, x, y, z, 1))
  deformSphere(rightGeo, (x, y, z) => hemispherePoint(1, x, y, z, 1))
  deformSphere(cereGeo, (x, y, z) => cerebellumPoint(x, y, z))

  const solidMat = new THREE.MeshPhongMaterial({
    color: "#b7c9c9",
    transparent: true,
    opacity: 0.32,
    depthWrite: false,
    shininess: 28,
    specular: new THREE.Color("#6a8888"),
    side: THREE.FrontSide,
  })
  const cereMat = new THREE.MeshPhongMaterial({
    color: "#a9bdbd",
    transparent: true,
    opacity: 0.26,
    depthWrite: false,
    shininess: 16,
    specular: new THREE.Color("#5a7777"),
    side: THREE.FrontSide,
  })

  const left = new THREE.Mesh(leftGeo, solidMat)
  const right = new THREE.Mesh(rightGeo, solidMat)
  const cere = new THREE.Mesh(cereGeo, cereMat)
  left.renderOrder = 0
  right.renderOrder = 0
  cere.renderOrder = 0

  const group = new THREE.Group()
  group.add(left)
  group.add(right)
  group.add(cere)

  const paint = (dark: boolean) => {
    solidMat.color.set(dark ? "#1a3c3c" : "#b7c9c9")
    solidMat.opacity = dark ? 0.45 : 0.34
    solidMat.emissive.set("#0e6e6e")
    solidMat.emissiveIntensity = dark ? 0.1 : 0.04
    cereMat.color.set(dark ? "#152f2f" : "#a9bdbd")
    cereMat.opacity = dark ? 0.38 : 0.26
    cereMat.emissive.set("#0e6e6e")
    cereMat.emissiveIntensity = dark ? 0.06 : 0.02
  }

  const dispose = () => {
    leftGeo.dispose()
    rightGeo.dispose()
    cereGeo.dispose()
    solidMat.dispose()
    cereMat.dispose()
  }

  return { group, paint, dispose }
}

function graphLights(dark: boolean): THREE.Light[] {
  const ambient = new THREE.AmbientLight(dark ? 0x8aa4a4 : 0xffffff, dark ? 0.55 : 0.62)
  const key = new THREE.DirectionalLight(dark ? 0xd5ecec : 0xffffff, dark ? 0.7 : 0.85)
  key.position.set(50, 90, 70)
  const fill = new THREE.DirectionalLight(dark ? 0x3d6a6a : 0xb7c8c8, dark ? 0.4 : 0.32)
  fill.position.set(-70, 10, -40)
  const rim = new THREE.DirectionalLight(dark ? 0x37a8a0 : 0x0e6e6e, dark ? 0.22 : 0.12)
  rim.position.set(0, -40, -80)
  return [ambient, key, fill, rim]
}

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
  const framed = useRef(false)
  const cortex = useRef<Cortex | null>(null)
  const geos = useRef<{ core: THREE.SphereGeometry; halo: THREE.SphereGeometry } | null>(null)
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
        return dark ? "#1c2c2f" : "#e4eaeb"
      }
      return l.type === "quoted_in" ? (dark ? "#2a3c40" : "#d5dedf") : dark ? "#4d656c" : "#8aa0a6"
    })
      .linkWidth((l) => {
        if (focus && linkTouches(l, focus)) return l.type === "quoted_in" ? 0.9 : 1.8
        return l.type === "quoted_in" ? 0.25 : 0.55
      })
      .linkOpacity(focus ? 0.9 : 0.42)
      .linkDirectionalParticles((l) => {
        if (!focus || !linkTouches(l, focus)) return 0
        return l.type === "quoted_in" ? 2 : 3
      })
      .linkDirectionalParticleWidth(1.4)
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

    for (const n of nodes) {
      const b = built.current.get(n.id)
      if (!b) continue
      const isFocus = n.id === focus
      const near = neighbours.has(n.id)
      let opacity = 1
      if (!matches(n)) opacity = 0.1
      else if (focus && !isFocus && !near) opacity = 0.28
      const color = BRAIN_KIND_COLOR[n.kind] ?? BRAIN_KIND_COLOR.document
      b.core.material.color.set(color)
      b.core.material.emissive.set(isFocus ? color : near ? color : "#000000")
      b.core.material.emissiveIntensity = isFocus ? 0.55 : near ? 0.22 : 0.08
      b.core.material.opacity = opacity
      b.halo.material.color.set(color)
      b.halo.material.opacity = opacity * (isFocus ? 0.28 : near ? 0.18 : 0.1)
      const scale = isFocus ? 1.4 : near ? 1.18 : 1
      b.core.scale.setScalar(scale)
      b.halo.scale.setScalar(scale)
      b.label.visible = isFocus || near
      b.label.material.opacity = opacity
    }

    applyLinkStyle()
  }

  const frameBrain = (ms: number) => {
    const g = graph.current
    if (!g) return
    const home = brainCameraHome()
    g.cameraPosition(home.position, home.lookAt, ms)
  }

  useEffect(() => {
    const el = host.current
    if (!el) return
    const builtMap = built.current
    const cacheMap = cache.current

    const g = new (ForceGraph3D as unknown as GraphCtor)(el, {
      controlType: "orbit",
      rendererConfig: { antialias: true, alpha: false, powerPreference: "high-performance" },
    })
    graph.current = g
    g.renderer().setPixelRatio(Math.min(window.devicePixelRatio || 1, 2))

    const shared = {
      core: new THREE.SphereGeometry(1, 24, 16),
      halo: new THREE.SphereGeometry(1, 16, 12),
    }
    geos.current = shared

    const tone = () => {
      const css = getComputedStyle(document.documentElement)
      return {
        bg: css.getPropertyValue("--k-surface").trim() || "#ffffff",
        ink: css.getPropertyValue("--k-text").trim() || "#0f1619",
        dark: document.documentElement.classList.contains("dark"),
      }
    }
    let t = tone()

    const cortexBuilt = buildCortex()
    cortexBuilt.paint(t.dark)
    cortex.current = cortexBuilt
    g.scene().add(cortexBuilt.group)
    g.lights(graphLights(t.dark))

    g.backgroundColor(t.bg)
      .showNavInfo(false)
      .width(el.clientWidth)
      .height(el.clientHeight)
      .nodeThreeObject((n) => {
        const r = BRAIN_KIND_RADIUS[n.kind] ?? 3
        const color = BRAIN_KIND_COLOR[n.kind] ?? BRAIN_KIND_COLOR.document
        const core = new THREE.Mesh(
          shared.core,
          new THREE.MeshPhongMaterial({
            color,
            transparent: true,
            opacity: 1,
            shininess: 36,
            specular: new THREE.Color("#ffffff"),
            emissive: color,
            emissiveIntensity: 0.08,
          }),
        )
        core.scale.setScalar(r)
        const halo = new THREE.Mesh(
          shared.halo,
          new THREE.MeshPhongMaterial({
            color,
            transparent: true,
            opacity: 0.12,
            depthWrite: false,
            shininess: 4,
            emissive: color,
            emissiveIntensity: 0.15,
          }),
        )
        halo.scale.setScalar(r * 1.75)
        const label = new SpriteText(n.title.length > 34 ? `${n.title.slice(0, 33)}…` : n.title)
        label.material.depthWrite = false
        label.material.transparent = true
        label.color = t.ink
        label.backgroundColor = t.dark ? "rgba(13,19,21,0.78)" : "rgba(255,255,255,0.88)"
        label.padding = 1.4
        label.borderRadius = 2.5
        label.textHeight = n.kind === "document" ? 3.2 : 4.2
        label.position.y = -(r + 7)
        label.visible = false
        const group = new THREE.Group()
        group.add(halo)
        group.add(core)
        group.add(label)
        built.current.set(n.id, { core, halo, label })
        return group
      })
      .nodeLabel(() => "")
      .linkLabel((l) => {
        const s = l.source as GNode
        const d = l.target as GNode
        return `${s.title} ${l.label} ${d.title}`
      })
      .linkCurvature((l) => (l.type === "quoted_in" ? 0.18 : 0.32))
      .linkCurveRotation((l) => {
        const a = String((l.source as GNode).id)
        const b = String((l.target as GNode).id)
        return unitHash(`${a}|${b}`, 3) * Math.PI * 2
      })
      .linkDirectionalArrowLength((l) => (l.type === "quoted_in" ? 0 : 2.2))
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
      })
      .onBackgroundClick(() => state.current.onSelect(null))
      .enableNodeDrag(false)
      .warmupTicks(0)
      .cooldownTicks(0)

    applyLinkStyle()
    frameBrain(0)

    const ro = new ResizeObserver(() => {
      g.width(el.clientWidth).height(el.clientHeight)
    })
    ro.observe(el)

    const mo = new MutationObserver(() => {
      t = tone()
      g.backgroundColor(t.bg)
      g.lights(graphLights(t.dark))
      cortex.current?.paint(t.dark)
      for (const b of built.current.values()) {
        b.label.color = t.ink
        b.label.backgroundColor = t.dark ? "rgba(13,19,21,0.78)" : "rgba(255,255,255,0.88)"
      }
      applyLinkStyle()
    })
    mo.observe(document.documentElement, { attributes: true, attributeFilter: ["class"] })

    return () => {
      ro.disconnect()
      mo.disconnect()
      g.scene().remove(cortexBuilt.group)
      cortexBuilt.dispose()
      cortex.current = null
      shared.core.dispose()
      shared.halo.dispose()
      geos.current = null
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
    const layout = layoutBrainNodes(nodes, edges)
    const next = new Map<string, GNode>()
    for (const n of nodes) {
      const p = layout.get(n.id) ?? { x: 0, y: 0, z: 0 }
      const prev = cache.current.get(n.id)
      if (prev) {
        prev.title = n.title
        prev.kind = n.kind
        prev.status = n.status
        prev.x = p.x
        prev.y = p.y
        prev.z = p.z
        prev.fx = p.x
        prev.fy = p.y
        prev.fz = p.z
        next.set(n.id, prev)
      } else {
        next.set(n.id, { ...n, x: p.x, y: p.y, z: p.z, fx: p.x, fy: p.y, fz: p.z })
      }
    }
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
    g.graphData({ nodes: nodeList, links })
    if (!framed.current) {
      frameBrain(0)
      framed.current = true
    }
    const timers = [window.setTimeout(restyle, 40), window.setTimeout(restyle, 240)]
    return () => timers.forEach((id) => window.clearTimeout(id))
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [nodes, edges, kindFilter])

  useEffect(() => {
    restyle()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedId, kindFilter])

  useEffect(() => {
    if (resetSignal === 0) return
    frameBrain(600)
  }, [resetSignal])

  return (
    <div
      ref={host}
      className="brain-graph h-full w-full"
      aria-label="Interactive company knowledge graph shaped as a brain"
      role="img"
    />
  )
}
