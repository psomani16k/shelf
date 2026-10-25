export type Direction = "left" | "right"

/** px the finger must travel; shorter is a tap or a scroll. */
const SWIPE_MIN = 60
/** ms; slower is a drag or a scroll. */
const SWIPE_MAX_MS = 800

/** The direction of a finished touch, or null when it was a tap or mostly vertical. */
export function swipeDirection({ dx, dy, ms }: { dx: number; dy: number; ms: number }) {
  if (ms > SWIPE_MAX_MS || Math.abs(dx) < SWIPE_MIN || Math.abs(dx) < 2 * Math.abs(dy)) {
    return null
  }
  return dx < 0 ? "left" : "right"
}

/**
 * Turns a stream of horizontal wheel deltas into single steps. A touchpad swipe sends many small
 * deltas, so after a step the rest of the stream is ignored until it's been quiet for `quietMs`.
 */
export function wheelTurner({ threshold = 80, quietMs = 250 } = {}) {
  let sum = 0
  let fired = false
  let last = -Infinity
  return (deltaX: number, now: number): -1 | 0 | 1 => {
    if (now - last > quietMs) {
      sum = 0
      fired = false
    }
    last = now
    if (fired) return 0
    sum += deltaX
    if (Math.abs(sum) < threshold) return 0
    fired = true
    return sum < 0 ? -1 : 1
  }
}
