import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { addLocalLibrary, addRemoteLibrary, listLibraries, removeLibrary } from "@/api/app"

/** App only: which library this device opens. */

export const LIBRARIES_KEY = ["libraries"]

export function useLibraries() {
  return useQuery({
    queryKey: LIBRARIES_KEY,
    queryFn: listLibraries,
    staleTime: Infinity,
    refetchOnWindowFocus: false,
  })
}

export type NewLibrary = { kind: "local"; path: string } | { kind: "remote"; baseUrl: string }

export function useAddLibrary() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (library: NewLibrary) =>
      library.kind === "local" ? addLocalLibrary(library.path) : addRemoteLibrary(library.baseUrl),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: LIBRARIES_KEY }),
  })
}

/** Forgets the library and everything cached from it, so the app returns to the picker. */
export function useForgetLibrary() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: () => removeLibrary(0),
    onSuccess: () => {
      queryClient.removeQueries({ predicate: (q) => q.queryKey[0] !== LIBRARIES_KEY[0] })
      return queryClient.invalidateQueries({ queryKey: LIBRARIES_KEY })
    },
  })
}
