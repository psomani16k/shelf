import { useSyncExternalStore } from "react"

/** Whether `query` matches, kept up to date as the window or device changes. */
export function useMediaQuery(query: string): boolean {
  return useSyncExternalStore(
    (onChange) => {
      const list = matchMedia(query)
      list.addEventListener("change", onChange)
      return () => list.removeEventListener("change", onChange)
    },
    () => matchMedia(query).matches,
  )
}
