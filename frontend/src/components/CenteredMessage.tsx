import type { ReactNode } from "react"

/** A message centred in the space it's given, e.g. where a book would be. */
export function CenteredMessage({ children }: { children: ReactNode }) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-4 p-6 text-center">
      {children}
    </div>
  )
}
