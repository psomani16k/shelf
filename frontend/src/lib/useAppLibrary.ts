import { type QueryClient, useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
  addLocalLibrary,
  addRemoteLibrary,
  type LibraryInfo,
  listLibraries,
  removeLibrary,
  selectLibrary,
} from "@/api/app"

/** App only: the libraries this device knows and which one is open. */

export const LIBRARIES_KEY = ["libraries"]

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
 * Everything cached came from the previous library: its books, last book, covers and config.
 * Dropping it makes the next library load its own, and `AppFlow` opens it.
 */
function resetLibraryCache(queryClient: QueryClient) {
  queryClient.removeQueries({ predicate: (q) => q.queryKey[0] !== LIBRARIES_KEY[0] })
  return queryClient.invalidateQueries({ queryKey: LIBRARIES_KEY })
}

export function useSwitchLibrary() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: selectLibrary,
    onSuccess: () => resetLibraryCache(queryClient),
  })
}

export type NewLibrary =
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
