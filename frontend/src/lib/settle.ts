/** px/ms: a release faster than this settles in its direction regardless of position. */
const FLICK = 0.4

/** Whether a released drawer drag ends open. `velocity` is px/ms, positive towards open. */
export function settle(progress: number, velocity: number): boolean {
  if (Math.abs(velocity) > FLICK) return velocity > 0
  return progress > 0.5
}
