import { type QueryClient, useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
  addLocalLibrary,
  addRemoteLibrary,
  type LibraryInfo,
  listLibraries,
  removeLibrary,
  selectLibrary,
} from "@/api/app"
import { LIBRARIES_KEY, OPEN_LIBRARY_KEY, UI_SCALE_KEY } from "@/lib/queryKeys"
import { setSignedOut } from "@/lib/session"
import { uploads } from "@/lib/uploads"

export function useLibraries() {
  return useQuery({
    queryKey: LIBRARIES_KEY,
    queryFn: listLibraries,
    staleTime: Infinity,
    refetchOnWindowFocus: false,
  })
}

/** The stored current library, or the first one when that's unset or gone. */
export function useCurrentLibrary(): LibraryInfo | undefined {
  const data = useLibraries().data
  return data?.libraries.find((l) => l.name === data.current) ?? data?.libraries[0]
}

export function nameTaken(libraries: LibraryInfo[] | undefined, name: string): boolean {
  const wanted = name.trim().toLowerCase()
  return !!libraries?.some((l) => l.name.toLowerCase() === wanted)
}

/**
 * Everything cached, except the library list and the interface scale, came from the previous
 * library (or user): its books, last book, covers and config. Dropping it makes the next one
 * load its own, and `AppFlow` opens it. Files still waiting to upload would go to the next
 * library, so they're dropped too. Whether it needs a login comes from its `signed_in`.
 */
export function resetLibraryCache(queryClient: QueryClient) {
  return resetCache(queryClient, [LIBRARIES_KEY, UI_SCALE_KEY])
}

/**
 * After another user signs in to the same open library: drops everything cached for the last
 * one, but keeps the library open (reopening a local one would sync it again).
 */
export function resetUserCache(queryClient: QueryClient) {
  return resetCache(queryClient, [LIBRARIES_KEY, UI_SCALE_KEY, OPEN_LIBRARY_KEY])
}

/** Drops every query but those under `kept`, and queued uploads. */
function resetCache(queryClient: QueryClient, kept: string[][]) {
  uploads.dropQueued()
  setSignedOut(false)
  const prefixes = kept.map((key) => key[0])
  queryClient.removeQueries({ predicate: (q) => !prefixes.includes(q.queryKey[0] as string) })
  return queryClient.invalidateQueries({ queryKey: LIBRARIES_KEY })
}

export function useSwitchLibrary() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: selectLibrary,
    onSuccess: () => resetLibraryCache(queryClient),
  })
}

type NewLibrary =
  | { kind: "local"; name: string; path: string }
  | { kind: "remote"; name: string; baseUrl: string }

/** Adds the library and switches to it. */
export function useAddLibrary() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (library: NewLibrary) =>
      library.kind === "local"
        ? addLocalLibrary(library.name, library.path)
        : addRemoteLibrary(library.name, library.baseUrl),
    onSuccess: () => resetLibraryCache(queryClient),
  })
}

/** Forgets the library on this device. Removing the open one switches to the next. */
export function useRemoveLibrary() {
  const queryClient = useQueryClient()
  const current = useCurrentLibrary()
  return useMutation({
    mutationFn: removeLibrary,
    onSuccess: (_, name) =>
      name === current?.name
        ? resetLibraryCache(queryClient)
        : queryClient.invalidateQueries({ queryKey: LIBRARIES_KEY }),
  })
}
