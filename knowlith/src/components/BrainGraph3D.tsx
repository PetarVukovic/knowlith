import { Component, useEffect, useRef, type ReactNode } from "react"
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
  layoutBrainNodes,
} from "@/lib/brainGraph"
import type { BrainEdge, BrainNode } from "@/lib/types"

type GNode = NodeObject & BrainNode
type GLink = LinkObject<GNode> & { type: string; label: string }
type Graph = ForceGraph3DInstance<GNode, GLink>
type GraphCtor = new (el: HTMLElement, cfg?: ConfigOptions) => Graph

const ACCENT = "#0e6e6e"
const CONFLICT = "#e3776d"

type Built = {
  core: THREE.Mesh<THREE.BufferGeometry, THREE.MeshPhongMaterial>
  label: SpriteText
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

class GraphBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false }
  static getDerivedStateFromError() { return { failed: true } }
  render() {
    return this.state.failed
      ? <div className="grid h-full place-items-center p-8 text-center text-sm text-muted">3D is unavailable on this device. Your knowledge is available in the list.</div>
      : this.props.children
  }
}

export function BrainGraph3D(props: Parameters<typeof BrainScene>[0]) {
  return <GraphBoundary><BrainScene {...props} /></GraphBoundary>
}

function disposeNode(node: Built) {
  node.core.material.dispose()
  node.label.material.map?.dispose()
  node.label.material.dispose()
  node.label.geometry.dispose()
}

function BrainScene({
  nodes,
  edges,
  selectedId,
  kindFilter,
  resetSignal = 0,
  growing = false,
  onSelect,
  onOpen,
}: {
  nodes: BrainNode[]
  edges: BrainEdge[]
  selectedId: string | null
  kindFilter: string
  resetSignal?: number
  growing?: boolean
  onSelect: (node: BrainNode | null) => void
  onOpen: (node: BrainNode) => void
}) {
  const host = useRef<HTMLDivElement>(null)
  const motion = useRef(false)
  const signature = useRef("")
  const graph = useRef<Graph | null>(null)
  const cache = useRef(new Map<string, GNode>())
  const built = useRef(new Map<string, Built>())
  const hovered = useRef<string | null>(null)
  const framed = useRef(false)
  const nodesRef = useRef(nodes)
  nodesRef.current = nodes
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
      return l.type === "quoted_in" ? (dark ? "#63858a" : "#9dabad") : dark ? "#4d656c" : "#8aa0a6"
    })
      .linkWidth((l) => {
        if (focus && linkTouches(l, focus)) return l.type === "quoted_in" ? 0.9 : 1.8
        return l.type === "quoted_in" ? 0.25 : 0.55
      })
      .linkOpacity(focus ? 0.9 : 0.65)
      .linkDirectionalParticles((l) => {
        if (motion.current || !focus || !linkTouches(l, focus)) return 0
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

    for (const n of nodesRef.current) {
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
      const scale = isFocus ? 1.4 : near ? 1.18 : 1
      const radius = BRAIN_KIND_RADIUS[n.kind] ?? 3
      b.core.scale.setScalar(radius * scale)
      b.label.visible = isFocus || (near && neighbours.size <= 8) || (!focus && nodesRef.current.length <= 10)
      b.label.text = n.title.length > 34 ? `${n.title.slice(0, 33)}…` : n.title
      b.core.material.wireframe = false
      if (n.status === "draft" || n.status === "conflict") {
        b.core.material.wireframe = true
      }
      b.label.material.opacity = opacity
    }

    applyLinkStyle()
  }

  const frameBrain = (ms: number) => {
    const g = graph.current
    if (!g) return
    const visible = g.graphData().nodes
    if (visible.length) {
      // Library fit includes hidden label textures in its bounding box,
      // making a small graph almost invisible. Frame node coordinates only.
      const xs = visible.map((n) => n.x ?? 0)
      const ys = visible.map((n) => n.y ?? 0)
      const zs = visible.map((n) => n.z ?? 0)
      const range = (values: number[]) => values.reduce(([lo, hi], v) => [Math.min(lo, v), Math.max(hi, v)], [Infinity, -Infinity])
      const [xmin, xmax] = range(xs), [ymin, ymax] = range(ys), [zmin, zmax] = range(zs)
      const center = { x: (xmin + xmax) / 2, y: (ymin + ymax) / 2, z: (zmin + zmax) / 2 }
      const camera = g.camera() as THREE.PerspectiveCamera
      const halfFov = Math.tan(camera.fov * Math.PI / 360)
      const distance = Math.max(Math.max(80, xmax - xmin) / (2 * halfFov * camera.aspect * 0.65), Math.max(80, ymax - ymin) / (2 * halfFov * 0.65)) + (zmax - zmin) / 2
      g.cameraPosition({ x: center.x, y: center.y, z: center.z + distance }, center, motion.current ? 0 : ms)
    } else {
      const home = brainCameraHome()
      g.cameraPosition(home.position, home.lookAt, motion.current ? 0 : ms)
    }
  }

  useEffect(() => {
    const el = host.current
    if (!el) return
    const media = window.matchMedia("(prefers-reduced-motion: reduce)")
    motion.current = media.matches
    const motionChanged = () => { motion.current = media.matches; applyLinkStyle() }
    media.addEventListener("change", motionChanged)
    const builtMap = built.current
    const cacheMap = cache.current

    const g = new (ForceGraph3D as unknown as GraphCtor)(el, {
      controlType: "orbit",
      rendererConfig: { antialias: true, alpha: false, powerPreference: "high-performance" },
    })
    graph.current = g
    g.renderer().setPixelRatio(Math.min(window.devicePixelRatio || 1, 2))

    const shared = {
      core: new THREE.SphereGeometry(1, 16, 12),
      draft: new THREE.OctahedronGeometry(1),
    }

    const tone = () => {
      const css = getComputedStyle(document.documentElement)
      return {
        bg: css.getPropertyValue("--k-surface").trim() || "#ffffff",
        ink: css.getPropertyValue("--k-text").trim() || "#0f1619",
        dark: document.documentElement.classList.contains("dark"),
      }
    }
    let t = tone()

    g.lights(graphLights(t.dark))

    g.backgroundColor(t.bg)
      .showNavInfo(false)
      .width(el.clientWidth)
      .height(el.clientHeight)
      .nodeThreeObject((n) => {
        const existing = built.current.get(n.id)
        if (existing?.core.parent) return existing.core.parent
        const r = BRAIN_KIND_RADIUS[n.kind] ?? 3
        const color = BRAIN_KIND_COLOR[n.kind] ?? BRAIN_KIND_COLOR.document
        const core = new THREE.Mesh(
          n.status === "draft" || n.status === "conflict" ? shared.draft : shared.core,
          new THREE.MeshPhongMaterial({
            color,
            transparent: true,
            opacity: 1,
            shininess: 8,
            specular: new THREE.Color("#ffffff"),
            emissive: color,
            emissiveIntensity: 0.08,
          }),
        )
        core.scale.setScalar(r)
        const label = new SpriteText(n.title.length > 34 ? `${n.title.slice(0, 33)}…` : n.title)
        label.material.depthWrite = false
        label.material.depthTest = false
        label.fontSize = 80
        label.material.transparent = true
        label.color = t.ink
        label.backgroundColor = t.dark ? "rgba(13,19,21,0.78)" : "rgba(255,255,255,0.88)"
        label.padding = 1.4
        label.borderRadius = 2.5
        label.textHeight = n.kind === "document" ? 5 : 6
        label.position.y = -(r + 7)
        label.visible = false
        const group = new THREE.Group()
        group.add(core)
        group.add(label)
        built.current.set(n.id, { core, label })
        return group
      })
      .nodeLabel(() => "")
      .linkLabel((l) => {
        const s = l.source as GNode
        const d = l.target as GNode
        return `${s.title} ${l.label} ${d.title}`
      })
      .linkCurvature(0)
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

    const visibility = () => { if (document.hidden) g.pauseAnimation(); else g.resumeAnimation() }
    document.addEventListener("visibilitychange", visibility)
    visibility()
    const ro = new ResizeObserver(() => {
      g.width(el.clientWidth).height(el.clientHeight)
    })
    ro.observe(el)

    const mo = new MutationObserver(() => {
      t = tone()
      g.backgroundColor(t.bg)
      g.lights(graphLights(t.dark))
      for (const b of built.current.values()) {
        b.label.color = t.ink
        b.label.backgroundColor = t.dark ? "rgba(13,19,21,0.78)" : "rgba(255,255,255,0.88)"
      }
      applyLinkStyle()
    })
    mo.observe(document.documentElement, { attributes: true, attributeFilter: ["class"] })

    return () => {
      media.removeEventListener("change", motionChanged)
      document.removeEventListener("visibilitychange", visibility)
      for (const node of builtMap.values()) disposeNode(node)
      ro.disconnect()
      mo.disconnect()
      shared.core.dispose()
      shared.draft.dispose()
      g._destructor()
      graph.current = null
      signature.current = ""
      framed.current = false
      builtMap.clear()
      cacheMap.clear()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  useEffect(() => {
    const g = graph.current
    if (!g) return
    const nextSignature = JSON.stringify([nodes, edges, kindFilter])
    if (nextSignature === signature.current) return
    signature.current = nextSignature
    const layout = layoutBrainNodes(nodes, edges)
    const next = new Map<string, GNode>()
    for (const n of nodes) {
      const p = layout.get(n.id) ?? { x: 0, y: 0, z: 0 }
      const prev = cache.current.get(n.id)
      if (prev) {
        prev.title = n.title
        prev.kind = n.kind
        prev.status = n.status
        // Existing nodes keep their place while new evidence arrives.
        const mesh = built.current.get(n.id)
        if (mesh) mesh.label.text = n.title.length > 34 ? `${n.title.slice(0, 33)}…` : n.title
        next.set(n.id, prev)
      } else {
        next.set(n.id, { ...n, x: p.x, y: p.y, z: p.z, fx: p.x, fy: p.y, fz: p.z })
      }
    }
    cache.current = next
    for (const id of [...built.current.keys()]) {
      if (!next.has(id)) {
        const mesh = built.current.get(id)
        if (mesh) disposeNode(mesh)
        built.current.delete(id)
      }
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
    const firstFrame = !framed.current && nodeList.length > 0
    if (firstFrame) framed.current = true
    const timers = [window.setTimeout(restyle, 40), window.setTimeout(() => {
      restyle()
      if (firstFrame) frameBrain(0)
    }, 240)]
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
      aria-label={growing ? "Company brain forming from extracted documents and discoveries" : "Interactive 3D company knowledge graph"}
      role="img"
    />
  )
}
