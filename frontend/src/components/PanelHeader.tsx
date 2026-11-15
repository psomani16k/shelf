import { X } from "lucide-react"
import type { ReactNode } from "react"
import { Button } from "@/components/ui/button"

/** A drawer panel's title and close button. `children` go under the title. */
export function PanelHeader({
  title,
  onClose,
  children,
}: {
  title: string
  onClose: () => void
  children?: ReactNode
}) {
  return (
    <div className={`flex ${children ? "items-start" : "items-center"} gap-2 border-b py-3 pr-2 pl-4`}>
      {children ? (
        <div className="min-w-0 flex-1">
          <h2 className="font-medium">{title}</h2>
          {children}
        </div>
      ) : (
        <h2 className="flex-1 font-medium">{title}</h2>
      )}
      <Button
        variant="ghost"
        size="icon"
        onClick={onClose}
        aria-label={`Close ${title.toLowerCase()}`}
      >
        <X />
      </Button>
    </div>
  )
}
