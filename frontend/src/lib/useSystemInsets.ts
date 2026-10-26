import { useEffect } from "react"
import { systemInsets } from "@/api/app"
import { IS_TAURI } from "@/lib/platform"

/**
 * App only: publishes the phone's status bar, navigation bar and cutout sizes as `--inset-*` CSS
 * variables on `<html>`, which the `safe-area` utility pads with. Re-read on every resize, which
 * covers rotation.
 */
export function useSystemInsets() {
  useEffect(() => {
    if (!IS_TAURI) return
    const apply = () => {
      void systemInsets().then((insets) => {
        const style = document.documentElement.style
        for (const side of ["top", "right", "bottom", "left"] as const) {
          style.setProperty(`--inset-${side}`, `${insets[side]}px`)
        }
      })
    }
    apply()
    window.addEventListener("resize", apply)
    return () => window.removeEventListener("resize", apply)
  }, [])
}
