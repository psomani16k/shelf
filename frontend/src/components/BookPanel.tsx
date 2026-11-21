import type { ReactNode } from "react"
import { PanelHeader } from "@/components/PanelHeader"
import { cn } from "cn"

export type BookTab = "contents" | "highlights"

const TABS: { id: BookTab; label: string }[] = [
  { id: "contents", label: "Contents" },
  { id: "highlights", label: "Highlights" },
]

/** The right drawer: the tab's title and close, its body, then the tabs, centred at the bottom. */
export function BookPanel({
  tab,
  onTabChange,
  onClose,
  children,
}: {
  tab: BookTab
  onTabChange: (tab: BookTab) => void
  onClose: () => void
  children: ReactNode
}) {
  const title = TABS.find((t) => t.id === tab)?.label ?? ""
  return (
    <>
      <PanelHeader title={title} onClose={onClose} />
      <div role="tabpanel" aria-label={title} className="flex min-h-0 flex-1 flex-col">
        {children}
      </div>
      <div className="flex justify-center border-t p-2">
        <div role="tablist" className="flex gap-1 rounded-lg bg-muted p-1">
          {TABS.map((t) => (
            <button
              key={t.id}
              type="button"
              role="tab"
              aria-selected={tab === t.id}
              onClick={() => onTabChange(t.id)}
              className={cn(
                "rounded-md px-4 py-1.5 text-sm font-medium",
                tab === t.id
                  ? "bg-background text-foreground shadow-sm"
                  : "text-muted-foreground hover:text-foreground",
              )}
            >
              {t.label}
            </button>
          ))}
        </div>
      </div>
    </>
  )
}
