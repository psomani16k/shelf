import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useEffect, useMemo } from "react"
import { deleteTheme, getConfig, listThemes, setConfig, setTheme } from "@/api"
import type { ThemeEntry } from "@/gen/apipb/theme_pb"
import { ReaderImages_Value } from "@/gen/configpb/config_pb"
import { type Appearance, resolveAppearance } from "@/lib/appearance"
import { LIBRARY_CONFIG_KEY, THEMES_KEY } from "@/lib/queryKeys"
import { type ThemeLook, themeLook } from "@/lib/readerTheme"
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

/** Read once on load, like the config; theme changes write their result into this cache. */
export function useReaderThemes() {
  return useQuery({
    queryKey: THEMES_KEY,
    queryFn: listThemes,
    staleTime: Infinity,
    refetchOnWindowFocus: false,
  })
}

/** Creates or replaces one of the user's themes, and puts the new list in the cache. */
export function useSaveTheme() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: setTheme,
    onSuccess: (list) => queryClient.setQueryData(THEMES_KEY, list),
  })
}

/** Deletes one of the user's themes by name, and puts the new list in the cache. */
export function useDeleteTheme() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: deleteTheme,
    onSuccess: (list) => queryClient.setQueryData(THEMES_KEY, list),
  })
}

/** The theme the config names, else the list's default; `undefined` until both load. */
export function useReaderTheme(): ThemeEntry | undefined {
  const config = useLibraryConfig().data
  const list = useReaderThemes().data
  if (!config || !list) return undefined
  const find = (id: string | undefined) => list.themes.find((t) => t.id === id)
  return find(config.reader?.theme) ?? find(list.defaultId)
}

/** The chosen theme as filters; the book as published until the theme is known. */
export function useReaderLook(): ThemeLook {
  const spec = useReaderTheme()?.spec
  const images = useLibraryConfig().data?.reader?.images
  const keep = (images ?? ReaderImages_Value.KEEP) === ReaderImages_Value.KEEP
  return useMemo(() => themeLook(spec, keep ? "keep" : "page"), [spec, keep])
}
