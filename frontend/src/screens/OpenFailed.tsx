import { useState } from "react"
import type { LibraryInfo } from "@/api/app"
import { CenteredScreen } from "@/components/CenteredScreen"
import { LibrarySwitcher } from "@/components/LibrarySwitcher"
import { RemoveLibraryConfirm } from "@/components/RemoveLibraryConfirm"
import { Button } from "@/components/ui/button"
import { useLibraries } from "@/lib/useAppLibrary"

/** App only: the library list couldn't be read, or the current library couldn't be opened. */
export function OpenFailed({
  library,
  error,
  onRetry,
}: {
  library?: LibraryInfo
  error: unknown
  onRetry: () => void
}) {
  const others = (useLibraries().data?.libraries.length ?? 0) > 1
  const [removing, setRemoving] = useState(false)

  return (
    <CenteredScreen>
      <h1 className="text-lg font-medium">
        {library ? `Couldn't open ${library.name}` : "Couldn't load your libraries"}
      </h1>
      <p className="max-w-sm text-sm text-destructive">{String(error)}</p>
      <div className="flex flex-wrap justify-center gap-2">
        <Button onClick={onRetry}>Try again</Button>
        {library && !removing && (
          <Button variant="outline" onClick={() => setRemoving(true)}>
            Remove this library
          </Button>
        )}
      </div>
      {library && removing && (
        <div className="w-full max-w-sm rounded-lg border p-4">
          <RemoveLibraryConfirm library={library} onCancel={() => setRemoving(false)} />
        </div>
      )}
      {others && (
        <div className="flex w-full max-w-sm flex-col gap-1 text-left">
          <p className="text-sm text-muted-foreground">Or open another library:</p>
          <LibrarySwitcher className="rounded-lg border" />
        </div>
      )}
    </CenteredScreen>
  )
}
