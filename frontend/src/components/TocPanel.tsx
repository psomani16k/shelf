import type { TocItem } from "foliate-js/view.js"
import { ChevronRight, X } from "lucide-react"
import { useEffect, useMemo, useRef, useState } from "react"
import { Button } from "@/components/ui/button"
import { tocPath } from "@/lib/toc"
import { cn } from "cn"

type Props = {
  toc: TocItem[]
  /** The entry the reader is in, highlighted and scrolled into view on open. */
  currentId: number | undefined
  open: boolean
  onPick: (href: string) => void
  onClose: () => void
}

export function TocPanel({ toc, currentId, open, onPick, onClose }: Props) {
  const listRef = useRef<HTMLDivElement>(null)
  // Entries on the way to the current chapter start expanded; the rest start collapsed.
  const path = useMemo(() => new Set(tocPath(toc, currentId)), [toc, currentId])
  // Only what the reader toggled by hand; everything else follows `path`.
  const [toggled, setToggled] = useState(() => new Map<number, boolean>())

  const isExpanded = (id: number) => toggled.get(id) ?? path.has(id)
  const toggle = (id: number) => setToggled((prev) => new Map(prev).set(id, !isExpanded(id)))

  useEffect(() => {
    const list = listRef.current
    if (!open || !list) return
    const current = list.querySelector<HTMLElement>("[aria-current]")
    // Not scrollIntoView: it would also scroll the drawer's clipped root.
    if (current) list.scrollTop = current.offsetTop - (list.clientHeight - current.offsetHeight) / 2
  }, [open])

  return (
    <>
      <div className="flex items-center gap-2 border-b py-3 pr-2 pl-4">
        <h2 className="flex-1 font-medium">Contents</h2>
        <Button variant="ghost" size="icon" onClick={onClose} aria-label="Close contents">
          <X />
        </Button>
      </div>

      <div ref={listRef} className="relative flex-1 overflow-y-auto overscroll-contain p-2">
        {toc.length === 0 ? (
          <p className="mt-8 text-center text-muted-foreground">
            This book has no table of contents.
          </p>
        ) : (
          <TocList
            items={toc}
            depth={0}
            tree={{ currentId, path, isExpanded, toggle, onPick }}
          />
        )}
      </div>
    </>
  )
}

type Tree = {
  currentId: number | undefined
  path: Set<number>
  isExpanded: (id: number) => boolean
  toggle: (id: number) => void
  onPick: (href: string) => void
}

function TocList({ items, depth, tree }: { items: TocItem[]; depth: number; tree: Tree }) {
  return (
    <ol>
      {items.map((item) => {
        const parent = !!item.subitems?.length
        const expanded = parent && tree.isExpanded(item.id)
        // With its parent collapsed, the current chapter is hidden, so the parent stands in for it.
        const current =
          item.id === tree.currentId || (parent && !expanded && tree.path.has(item.id))
        const label = item.label.trim()

        return (
          <li key={item.id}>
            <div
              className={cn(
                "flex items-start rounded-md",
                current && "bg-accent font-medium text-accent-foreground",
              )}
              style={{ paddingLeft: `${depth * 1.25}rem` }}
            >
              {parent ? (
                <button
                  type="button"
                  onClick={() => tree.toggle(item.id)}
                  aria-expanded={expanded}
                  aria-label={`${expanded ? "Collapse" : "Expand"} ${label}`}
                  className="flex size-9 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-muted"
                >
                  <ChevronRight
                    className={cn("size-4 transition-transform", expanded && "rotate-90")}
                  />
                </button>
              ) : (
                <span className="w-9 shrink-0" />
              )}
              <button
                type="button"
                // A heading with no link of its own just opens or closes its group.
                disabled={!item.href && !parent}
                onClick={() => (item.href ? tree.onPick(item.href) : tree.toggle(item.id))}
                aria-current={current ? "location" : undefined}
                className="min-w-0 flex-1 rounded-md px-3 py-2 text-left text-sm enabled:hover:bg-muted disabled:text-muted-foreground"
              >
                <span className="line-clamp-2">{label}</span>
              </button>
            </div>
            {expanded && <TocList items={item.subitems ?? []} depth={depth + 1} tree={tree} />}
          </li>
        )
      })}
    </ol>
  )
}
