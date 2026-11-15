import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useEffect } from "react"
import { getConfig, setConfig } from "@/api"
import { type Appearance, resolveAppearance } from "@/lib/appearance"
import { LIBRARY_CONFIG_KEY } from "@/lib/queryKeys"
import { useMediaQuery } from "@/lib/useMediaQuery"

/** Read once on load; `setConfig` results are written straight into this cache. */
export function useLibraryConfig() {
  return useQuery({
    queryKey: LIBRARY_CONFIG_KEY,
    queryFn: getConfig,
    staleTime: Infinity,
    refetchOnWindowFocus: false,
  })
}

/** Saves a partial config and puts the merged result in the cache. */
export function useSetConfig() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: setConfig,
    onSuccess: (updated) => queryClient.setQueryData(LIBRARY_CONFIG_KEY, updated),
  })
}

/** Resolves the user's config for this library and applies the UI theme to the page. */
export function useAppearance(): Appearance {
  const config = useLibraryConfig().data
  const systemDark = useMediaQuery("(prefers-color-scheme: dark)")
  const appearance = resolveAppearance(config, systemDark ? "dark" : "light")

  useEffect(() => {
    document.documentElement.classList.toggle("dark", appearance.ui === "dark")
  }, [appearance.ui])

  return appearance
}
