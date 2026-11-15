import type { ReactNode } from "react"
import { cn } from "cn"

/** A setting's title and description, with its control beside them (below on phones). */
export function ActionRow({
  title,
  description,
  status,
  error,
  className,
  children,
}: {
  title: string
  description?: ReactNode
  status?: ReactNode
  error?: ReactNode
  className?: string
  children: ReactNode
}) {
  return (
    <div
      className={cn(
        "flex flex-col gap-3 py-4 sm:flex-row sm:items-center sm:justify-between sm:gap-6",
        className,
      )}
    >
      <div className="min-w-0">
        <p className="text-sm font-medium">{title}</p>
        {description && <p className="text-sm text-muted-foreground">{description}</p>}
        {status && <p className="mt-1 text-sm">{status}</p>}
        {error && <p className="mt-1 text-sm text-destructive">{error}</p>}
      </div>
      {children}
    </div>
  )
}
