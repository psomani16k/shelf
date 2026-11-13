import { useState } from "react"

/** 50% to 200% in 10% steps. The book also scales with the interface (`lib/uiScale.ts`). */
export const ZOOM_STEPS = Array.from({ length: 16 }, (_, i) => (50 + 10 * i) / 100)

const MIN = 0.5
const MAX = 2

// Per device, like browser zoom, so it lives here and not in the user's config.
const KEY = "shelf.reader-zoom"

export function loadZoom(): number {
  try {
    return snapZoom(Number(localStorage.getItem(KEY)))
  } catch {
    return 1
  }
}

/** The nearest step, so levels saved under older steps (e.g. 125%, 300%) still load. */
export function snapZoom(zoom: number): number {
  if (!Number.isFinite(zoom) || zoom <= 0) return 1
  return Math.min(MAX, Math.max(MIN, Math.round(zoom * 10) / 10))
}

function saveZoom(zoom: number) {
  try {
    localStorage.setItem(KEY, String(zoom))
  } catch {
    // Storage can be blocked; the zoom then lasts until the reader closes.
  }
}

/** The next level up (1) or down (-1), staying at the ends. */
export function stepZoom(zoom: number, dir: 1 | -1): number {
  const i = ZOOM_STEPS.indexOf(zoom)
  const next = i === -1 ? ZOOM_STEPS.indexOf(1) : i + dir
  return ZOOM_STEPS[Math.min(ZOOM_STEPS.length - 1, Math.max(0, next))] ?? 1
}

/** The book's zoom on this device, saved as it changes. */
export function useReaderZoom(): [number, (zoom: number) => void] {
  const [zoom, setZoom] = useState(loadZoom)
  return [
    zoom,
    (next) => {
      setZoom(next)
      saveZoom(next)
    },
  ]
}
