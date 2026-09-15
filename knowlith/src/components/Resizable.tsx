import { useCallback, useEffect, useRef, useState } from "react"
import { cn } from "@/lib/utils"

interface PanelSize {
  width: number
  setWidth: (w: number) => void
  reset: () => void
  min: number
  max: number
}

function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value))
}

/**
 * A panel width the user owns. Persisted per viewer so the layout they set up
 * is the layout they come back to; storage failures fall back to the default
 * rather than breaking the page.
 */
export function usePanelSize(key: string, initial: number, min: number, max: number): PanelSize {
  const [width, setWidthState] = useState(() => {
    try {
      const stored = localStorage.getItem(`knowlith.panel.${key}`)
      return stored ? clamp(Number(stored), min, max) : initial
    } catch {
      return initial
    }
  })

  const setWidth = useCallback(
    (next: number) => {
      const value = clamp(Math.round(next), min, max)
      setWidthState(value)
      try {
        localStorage.setItem(`knowlith.panel.${key}`, String(value))
      } catch {
        /* private window or blocked storage — the width still applies this session */
      }
    },
    [key, min, max],
  )

  const reset = useCallback(() => setWidth(initial), [setWidth, initial])

  return { width, setWidth, reset, min, max }
}

/**
 * The drag strip between two panes. `edge` says which side of the handle the
 * resized panel is on: "start" for a left panel, "end" for a right panel.
 */
export function ResizeHandle({
  panel,
  edge,
  label,
  className,
}: {
  panel: PanelSize
  edge: "start" | "end"
  label: string
  className?: string
}) {
  const [dragging, setDragging] = useState(false)
  const origin = useRef({ x: 0, width: 0 })

  useEffect(() => {
    if (!dragging) return
    const onMove = (e: PointerEvent) => {
      const delta = e.clientX - origin.current.x
      panel.setWidth(origin.current.width + (edge === "start" ? delta : -delta))
    }
    const stop = () => setDragging(false)
    window.addEventListener("pointermove", onMove)
    window.addEventListener("pointerup", stop)
    window.addEventListener("pointercancel", stop)
    const previousCursor = document.body.style.cursor
    const previousSelect = document.body.style.userSelect
    document.body.style.cursor = "col-resize"
    document.body.style.userSelect = "none"
    return () => {
      window.removeEventListener("pointermove", onMove)
      window.removeEventListener("pointerup", stop)
      window.removeEventListener("pointercancel", stop)
      document.body.style.cursor = previousCursor
      document.body.style.userSelect = previousSelect
    }
  }, [dragging, edge, panel])

  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={label}
      aria-valuenow={panel.width}
      aria-valuemin={panel.min}
      aria-valuemax={panel.max}
      tabIndex={0}
      onPointerDown={(e) => {
        e.preventDefault()
        origin.current = { x: e.clientX, width: panel.width }
        setDragging(true)
      }}
      onDoubleClick={panel.reset}
      onKeyDown={(e) => {
        const step = e.shiftKey ? 48 : 16
        if (e.key === "ArrowLeft") {
          e.preventDefault()
          panel.setWidth(panel.width + (edge === "start" ? -step : step))
        } else if (e.key === "ArrowRight") {
          e.preventDefault()
          panel.setWidth(panel.width + (edge === "start" ? step : -step))
        } else if (e.key === "Enter") {
          e.preventDefault()
          panel.reset()
        }
      }}
      title="Drag to resize · double-click to reset"
      className={cn(
        "group relative z-10 w-1 shrink-0 cursor-col-resize touch-none select-none bg-line transition-colors",
        "hover:bg-accent/40 focus-visible:bg-accent/60",
        dragging && "bg-accent/60",
        className,
      )}
    >
      {/* A wider invisible hit area than the 1px line the user sees. */}
      <span className="absolute inset-y-0 -left-1.5 -right-1.5 block" />
    </div>
  )
}
