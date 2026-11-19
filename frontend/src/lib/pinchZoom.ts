import { useEffect } from "react"
import { IS_ANDROID } from "@/lib/platform"

const VIEWPORT = "width=device-width, initial-scale=1.0"

/**
 * Android app only: pinch zoom is for the book, so everything else feels like an app rather than
 * a web page. The web keeps it everywhere, for accessibility. Turning it off also snaps a zoomed
 * page back to 1×.
 */
export function allowPinchZoom(allowed: boolean) {
  if (!IS_ANDROID) return
  document
    .querySelector('meta[name="viewport"]')
    ?.setAttribute(
      "content",
      allowed ? VIEWPORT : `${VIEWPORT}, minimum-scale=1, maximum-scale=1, user-scalable=no`,
    )
}

/** Allows pinch zoom while mounted and `allowed`. */
export function usePinchZoom(allowed: boolean) {
  useEffect(() => {
    allowPinchZoom(allowed)
    return () => allowPinchZoom(false)
  }, [allowed])
}
