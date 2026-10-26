import { type CSSProperties, type PointerEvent, type ReactNode, useEffect, useRef } from "react"
import { settle } from "@/lib/settle"

/** Pixels a pointer must move before we decide whether it's a drag or a scroll. */
const SLOP = 8
const EASE = "duration-300 ease-[cubic-bezier(0.32,0.72,0,1)]"

export type DrawerSide = "left" | "right"

type Panel = { label: string; content: ReactNode }

type Drag = {
  pointerId: number
  startX: number
  startY: number
  startProgress: number
  /** 1 for the left panel, -1 for the right; 0 until a drag from closed picks a direction. */
  side: number
  width: number
  dragging: boolean
  progress: number
  lastX: number
  lastTime: number
  velocity: number
}

type Props = {
  open: DrawerSide | null
  onOpenChange: (open: DrawerSide | null) => void
  left: Panel
  /** Leave out to disable the right panel, its edge swipe included. */
  right?: Panel
  header: ReactNode
  children: ReactNode
}

const progressOf = (open: DrawerSide | null) => (open === "left" ? 1 : open === "right" ? -1 : 0)

/**
 * Drawers on both sides that push the page aside and follow the finger: full width, or half in
 * landscape. `--p` runs from -1 (right open) to 1 (left open) and drives every transform, so a
 * drag updates one CSS variable instead of re-rendering.
 */
export function SlideDrawer({ open, onOpenChange, left, right, header, children }: Props) {
  const rootRef = useRef<HTMLDivElement>(null)
  const leftRef = useRef<HTMLElement>(null)
  const rightRef = useRef<HTMLElement>(null)
  const drag = useRef<Drag | null>(null)
  const suppressClick = useRef(false)
  const returnFocus = useRef<HTMLElement | null>(null)

  const setProgress = (p: number) => rootRef.current?.style.setProperty("--p", String(p))

  useEffect(() => {
    if (!open) {
      returnFocus.current?.focus({ preventScroll: true })
      returnFocus.current = null
      return
    }
    returnFocus.current ??= document.activeElement as HTMLElement | null
    const panel = open === "left" ? leftRef.current : rightRef.current
    // preventScroll: focusing a half-offscreen panel would scroll the clipped root.
    panel?.focus({ preventScroll: true })
  }, [open])

  useEffect(() => {
    if (!open) return
    const onKey = (e: KeyboardEvent) => {
      // Inert means something covers the drawer, and that gets the Escape.
      if (e.key === "Escape" && !rootRef.current?.closest("[inert]")) onOpenChange(null)
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [open, onOpenChange])

  const onPointerDown = (e: PointerEvent) => {
    suppressClick.current = false
    // Mouse users have the buttons, the scrim and Esc; dragging would fight text selection.
    if (e.pointerType === "mouse") return
    const now = performance.now()
    const start = progressOf(open)
    drag.current = {
      pointerId: e.pointerId,
      startX: e.clientX,
      startY: e.clientY,
      startProgress: start,
      side: start,
      // Both panels are the same width.
      width: leftRef.current?.offsetWidth || 1,
      dragging: false,
      progress: start,
      lastX: e.clientX,
      lastTime: now,
      velocity: 0,
    }
  }

  const onPointerMove = (e: PointerEvent) => {
    const d = drag.current
    if (!d || d.pointerId !== e.pointerId) return
    const dx = e.clientX - d.startX
    if (!d.dragging) {
      const dy = e.clientY - d.startY
      if (Math.hypot(dx, dy) < SLOP) return
      if (d.side === 0) d.side = dx > 0 ? 1 : -1
      // Mostly vertical is a scroll, and there may be no right panel to pull.
      if (Math.abs(dx) <= Math.abs(dy) || (d.side === -1 && !right)) {
        drag.current = null
        return
      }
      d.dragging = true
      e.currentTarget.setPointerCapture(e.pointerId)
      rootRef.current?.setAttribute("data-dragging", "")
    }
    const now = performance.now()
    if (now > d.lastTime) {
      const instant = (e.clientX - d.lastX) / (now - d.lastTime)
      d.velocity = d.velocity * 0.5 + instant * 0.5
    }
    d.lastX = e.clientX
    d.lastTime = now
    // Each drag stays on its own side, so one swipe can't cross from one panel to the other.
    const [min, max] = d.side === 1 ? [0, 1] : [-1, 0]
    d.progress = Math.min(max, Math.max(min, d.startProgress + dx / d.width))
    setProgress(d.progress)
  }

  const onPointerEnd = (e: PointerEvent) => {
    const d = drag.current
    if (!d || d.pointerId !== e.pointerId) return
    drag.current = null
    if (!d.dragging) return
    suppressClick.current = true
    rootRef.current?.removeAttribute("data-dragging")
    const sideOpen = settle(d.side * d.progress, d.side * d.velocity)
    const next =
      e.type === "pointercancel" ? open : sideOpen ? (d.side === 1 ? "left" : "right") : null
    setProgress(progressOf(next))
    if (next !== open) onOpenChange(next)
  }

  const dragHandlers = {
    onPointerDown,
    onPointerMove,
    onPointerUp: onPointerEnd,
    onPointerCancel: onPointerEnd,
  }

  const moves = `transition-[transform,opacity] ${EASE} group-data-dragging:transition-none motion-reduce:transition-none`
  // `--push` is the panel's share of the screen, so the page moves exactly as far as the panel.
  const pushed = "translateX(calc(var(--p) * var(--push) * 100%))"
  const panelClass = `absolute inset-y-0 flex w-full landscape:w-1/2 touch-pan-y flex-col bg-background outline-none ${moves}`

  return (
    <div
      ref={rootRef}
      className="group safe-area fixed inset-0 overflow-hidden bg-background [--push:1] landscape:[--push:0.5]"
      style={{ "--p": progressOf(open) } as CSSProperties}
      // A drag that ends over a button must not also click it.
      onClickCapture={(e) => {
        if (!suppressClick.current) return
        suppressClick.current = false
        e.preventDefault()
        e.stopPropagation()
      }}
    >
      {/* Absolute children fill this box, which the safe-area padding keeps clear of the system bars. */}
      <div className="relative size-full">
      <div
        className={`absolute inset-0 flex flex-col ${moves}`}
        style={{ transform: pushed }}
        inert={open !== null}
      >
        <div className="touch-pan-y" {...dragHandlers}>
          {header}
        </div>
        <div className="relative min-h-0 flex-1">
          {children}
          {!open && (
            <div className="absolute inset-y-0 left-0 z-10 w-4 touch-pan-y" {...dragHandlers} />
          )}
          {!open && right && (
            <div className="absolute inset-y-0 right-0 z-10 w-4 touch-pan-y" {...dragHandlers} />
          )}
        </div>
      </div>

      <div
        aria-hidden
        className={`absolute inset-0 touch-none bg-black ${moves} ${open ? "" : "pointer-events-none"}`}
        // max(p, -p) rather than abs(), which older WebViews lack.
        style={{ transform: pushed, opacity: "calc(max(var(--p), -1 * var(--p)) * 0.4)" }}
        onClick={() => onOpenChange(null)}
        {...dragHandlers}
      />

      <aside
        ref={leftRef}
        tabIndex={-1}
        aria-label={left.label}
        className={`left-0 ${panelClass}`}
        style={{ transform: "translateX(calc((max(var(--p), 0) - 1) * 100%))" }}
        inert={open !== "left"}
        {...dragHandlers}
      >
        {left.content}
      </aside>

      {right && (
        <aside
          ref={rightRef}
          tabIndex={-1}
          aria-label={right.label}
          className={`right-0 ${panelClass}`}
          style={{ transform: "translateX(calc((1 + min(var(--p), 0)) * 100%))" }}
          inert={open !== "right"}
          {...dragHandlers}
        >
          {right.content}
        </aside>
      )}
      </div>
    </div>
  )
}
