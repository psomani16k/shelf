import type { TocItem } from "foliate-js/view.js"

/** The ids from the top level down to `id`, inclusive; empty if it isn't in `items`. */
export function tocPath(items: TocItem[], id: number | undefined): number[] {
  if (id === undefined) return []
  for (const item of items) {
    if (item.id === id) return [id]
    const below = tocPath(item.subitems ?? [], id)
    if (below.length) return [item.id, ...below]
  }
  return []
}
