import type { ReactNode } from "react"
import { cn } from "cn"

type Props = {
  title: string
  /** Usually a back or menu button. */
  start?: ReactNode
  end?: ReactNode
  className?: string
}

/** The 48px title bar at the top of a screen. */
export function ScreenHeader({ title, start, end, className }: Props) {
  return (
    <header className={cn("flex h-12 shrink-0 items-center gap-2 border-b px-2", className)}>
      {start}
      <h1 className={cn("min-w-0 flex-1 truncate font-medium", !start && "pl-2")}>{title}</h1>
      {end}
    </header>
  )
}
