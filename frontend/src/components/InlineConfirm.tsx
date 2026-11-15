import type { ReactNode } from "react"
import { Button } from "@/components/ui/button"
import { cn } from "cn"

/** Asks in place, with *Cancel* and a red confirm button. `children` is the question. */
export function InlineConfirm({
  confirmLabel,
  onConfirm,
  onCancel,
  pending,
  error,
  className,
  children,
}: {
  confirmLabel: string
  onConfirm: () => void
  onCancel: () => void
  pending: boolean
  error?: ReactNode
  className?: string
  children: ReactNode
}) {
  return (
    <div className={cn("flex flex-col gap-3", className)}>
      <p className="text-sm">{children}</p>
      {error && <p className="text-sm text-destructive">{error}</p>}
      <div className="flex justify-end gap-2">
        <Button variant="ghost" onClick={onCancel} disabled={pending}>
          Cancel
        </Button>
        <Button variant="destructive" onClick={onConfirm} disabled={pending}>
          {confirmLabel}
        </Button>
      </div>
    </div>
  )
}
