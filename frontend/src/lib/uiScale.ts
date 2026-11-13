import { type QueryClient, useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { setUiScale, uiScale } from "@/api/app"
import { log } from "@/lib/log"
import { IS_TAURI } from "@/lib/platform"

/** Per device, so it survives a library switch. */
export const UI_SCALE_KEY = ["ui-scale"]

/** Percents, matching `validate_ui_scale` in `app/src/app_data.rs`. */
export const UI_SCALE_CHOICES = Array.from({ length: 16 }, (_, i) => 50 + 10 * i)

/** Tailwind sizes in rem, so the root font size scales the whole UI but not px insets or viewport units. */
function applyUiScale(percent: number) {
  document.documentElement.style.fontSize = percent === 100 ? "" : `${percent}%`
}

/** App only: read before the first render, so the UI doesn't paint at 100% first. */
export async function loadUiScale(queryClient: QueryClient) {
  try {
    const percent = await uiScale()
    queryClient.setQueryData(UI_SCALE_KEY, percent)
    applyUiScale(percent)
  } catch (e) {
    log.warn("couldn't read the interface scale", { error: String(e) })
  }
}

/** The interface scale as a factor, e.g. 1.2. Always 1 on the web. */
export function useUiScale(): number {
  const { data } = useQuery({
    queryKey: UI_SCALE_KEY,
    queryFn: uiScale,
    enabled: IS_TAURI,
    staleTime: Infinity,
    refetchOnWindowFocus: false,
  })
  return (data ?? 100) / 100
}

export function useSaveUiScale() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: setUiScale,
    onSuccess: (percent) => {
      queryClient.setQueryData(UI_SCALE_KEY, percent)
      applyUiScale(percent)
    },
  })
}
