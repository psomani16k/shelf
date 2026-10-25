import { Button } from "@/components/ui/button"
import { useForgetLibrary, useLibraries } from "@/lib/useAppLibrary"

/** App only: which library this device opens. Changing it returns to the picker. */
export function LibrarySection() {
  const libraries = useLibraries()
  const forget = useForgetLibrary()
  const current = libraries.data?.[0]

  return (
    <div className="flex flex-col gap-4">
      <div className="rounded-lg border px-4">
        <div className="flex flex-col gap-3 py-4 sm:flex-row sm:items-center sm:justify-between sm:gap-6">
          <div className="min-w-0">
            <p className="text-sm font-medium">{current?.name ?? "No library"}</p>
            {current && (
              <p className="truncate text-sm text-muted-foreground">
                {current.kind === "local" ? "Folder on this device: " : "Shelf server: "}
                {current.location}
              </p>
            )}
          </div>
          <Button
            variant="outline"
            className="shrink-0"
            onClick={() => forget.mutate()}
            disabled={forget.isPending}
          >
            Change library
          </Button>
        </div>
      </div>
      {forget.isError && <p className="text-sm text-destructive">{String(forget.error)}</p>}
    </div>
  )
}
