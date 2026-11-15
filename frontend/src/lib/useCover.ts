import { type QueryClient, useQuery } from "@tanstack/react-query"
import { getCover } from "@/api"
import { COVER_KEY, coverKey } from "@/lib/queryKeys"

/**
 * An object URL for the book's thumbnail, once it has loaded. `failed` means there is none, so
 * the caller can show a placeholder instead of waiting.
 */
export function useCover(id: string): { url: string | undefined; failed: boolean } {
  const query = useQuery({
    queryKey: coverKey(id),
    queryFn: async () => URL.createObjectURL(await getCover(id)),
    staleTime: Infinity,
    retry: false,
    refetchOnWindowFocus: false,
  })
  return { url: query.data, failed: query.isError }
}

/** Object URLs hold their blob in memory until revoked; do that when the cache lets one go. */
export function revokeCoversOnRemoval(queryClient: QueryClient): () => void {
  return queryClient.getQueryCache().subscribe((event) => {
    const { queryKey, state } = event.query
    if (event.type === "removed" && queryKey[0] === COVER_KEY[0] && typeof state.data === "string") {
      URL.revokeObjectURL(state.data)
    }
  })
}
