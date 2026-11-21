import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { addAnnotation, deleteAnnotation, listAnnotations, updateAnnotation } from "@/api"
import type { NewAnnotation } from "@/api/types"
import type { Annotation, HighlightColor_Value } from "@/gen/bookkeeperpb/annotation_pb"
import { annotationsKey } from "@/lib/queryKeys"

/**
 * The user's highlights in the book, as the backend last answered. Apart from the book file, so
 * an unreadable highlights file doesn't keep the book from opening.
 */
export function useHighlights(bookId: string) {
  return useQuery({
    queryKey: annotationsKey(bookId),
    queryFn: async () => (await listAnnotations(bookId)).annotations,
    staleTime: Infinity,
    refetchOnWindowFocus: false,
  })
}

/** The list after a change: a highlight as stored now, or the id of one that's gone. */
function applyChange(list: Annotation[], change: Annotation | string): Annotation[] {
  if (typeof change === "string") return list.filter((a) => a.id !== change)
  return list.some((a) => a.id === change.id)
    ? list.map((a) => (a.id === change.id ? change : a))
    : [...list, change]
}

/** Each change writes the backend's answer into the cache that the reader and the list draw. */
function useChange<Vars, Result extends Annotation | string>(
  bookId: string,
  run: (vars: Vars) => Promise<Result>,
) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: run,
    onSuccess: (change) =>
      queryClient.setQueryData<Annotation[]>(annotationsKey(bookId), (list) =>
        applyChange(list ?? [], change),
      ),
  })
}

export const useAddHighlight = (bookId: string) =>
  useChange(bookId, (highlight: NewAnnotation) => addAnnotation(bookId, highlight))

export const useUpdateHighlight = (bookId: string) =>
  useChange(
    bookId,
    ({ id, ...patch }: { id: string; note?: string; color?: HighlightColor_Value }) =>
      updateAnnotation(bookId, id, patch),
  )

/** Resolves to the id it removed. */
export const useDeleteHighlight = (bookId: string) =>
  useChange(bookId, async (id: string) => {
    await deleteAnnotation(bookId, id)
    return id
  })
