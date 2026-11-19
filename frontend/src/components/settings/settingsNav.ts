import { useLocation, useNavigate, useParams } from "react-router"

export const SETTINGS_PATH = "/settings"

/**
 * `depth` counts the history entries settings has pushed, so closing can step back past all of
 * them. It's missing after a reload or a deep link. `fromList` marks a section opened from the
 * phone list, and `fromParent` a page opened from its section or parent page, so their back
 * arrows can just go back.
 */
export type NavState = { depth?: number; fromList?: boolean; fromParent?: boolean } | null

/** Moves between a section's pages (`/settings/<section>/<page>`). */
export function useSettingsPage() {
  const navigate = useNavigate()
  const { section } = useParams()
  const state = useLocation().state as NavState
  const depth = state?.depth
  const sectionPath = `${SETTINGS_PATH}/${section}`
  return {
    open: (page: string) =>
      void navigate(`${sectionPath}/${page}`, {
        state: { depth: depth && depth + 1, fromParent: true },
      }),
    /** Up to `parent` (a page of the same section), or the section itself. */
    back: (parent?: string) =>
      void (state?.fromParent
        ? navigate(-1)
        : navigate(parent ? `${sectionPath}/${parent}` : sectionPath, { replace: true, state })),
  }
}
