import { type MouseEvent, type PointerEvent, useEffect, useRef } from "react"

/** px a finger may drift before the press counts as a scroll. */
const SLOP = 10

/**
 * Handlers that call `onLongPress` when a touch is held still for `ms`. The click that ends a long
 * press is swallowed, so it doesn't also open the item. Mouse and pen are ignored.
 */
export function useLongPress(onLongPress: () => void, ms = 500) {
  const timer = useRef<number | undefined>(undefined)
  const start = useRef({ x: 0, y: 0 })
  const fired = useRef(false)

  const cancel = () => {
    window.clearTimeout(timer.current)
    timer.current = undefined
  }
  useEffect(() => cancel, [])

  return {
    onPointerDown: (e: PointerEvent) => {
      if (e.pointerType !== "touch") return
      fired.current = false
      start.current = { x: e.clientX, y: e.clientY }
      cancel()
      timer.current = window.setTimeout(() => {
        fired.current = true
        onLongPress()
      }, ms)
    },
    onPointerMove: (e: PointerEvent) => {
      const { x, y } = start.current
      if (Math.hypot(e.clientX - x, e.clientY - y) > SLOP) cancel()
    },
    // A scroll taking over the touch arrives as `pointercancel`.
    onPointerUp: cancel,
    onPointerCancel: cancel,
    // Android's long-press menu.
    onContextMenu: (e: MouseEvent) => {
      if (timer.current !== undefined || fired.current) e.preventDefault()
    },
    onClickCapture: (e: MouseEvent) => {
      if (!fired.current) return
      fired.current = false
      e.preventDefault()
      e.stopPropagation()
    },
  }
}
