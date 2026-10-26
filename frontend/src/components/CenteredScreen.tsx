import type { ReactNode } from "react"

export function CenteredScreen({ children }: { children: ReactNode }) {
  return (
    <main className="safe-area flex min-h-svh flex-col">
      <div className="flex flex-1 flex-col items-center justify-center gap-4 p-6 text-center">
        {children}
      </div>
    </main>
  )
}
