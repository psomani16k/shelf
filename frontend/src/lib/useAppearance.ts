import { useQuery } from "@tanstack/react-query"
import { useEffect } from "react"
import { getConfig } from "@/api"
import { type Appearance, resolveAppearance } from "@/lib/appearance"
import { useMediaQuery } from "@/lib/useMediaQuery"

export const LIBRARY_CONFIG_KEY = ["library-config"]

/** Read once on load; `setConfig` results are written straight into this cache. */
export function useLibraryConfig() {
  return useQuery({
    queryKey: LIBRARY_CONFIG_KEY,
    queryFn: getConfig,
    staleTime: Infinity,
    refetchOnWindowFocus: false,
  })
}

/** Resolves the library config and applies the UI theme to the page. */
export function useAppearance(): Appearance {
  const config = useLibraryConfig().data
  const systemDark = useMediaQuery("(prefers-color-scheme: dark)")
  const appearance = resolveAppearance(config, systemDark ? "dark" : "light")

  useEffect(() => {
    document.documentElement.classList.toggle("dark", appearance.ui === "dark")
  }, [appearance.ui])

  return appearance
}
