import { useMutation, useQueryClient } from "@tanstack/react-query"
import { CircleMinus, LoaderCircle, Plus } from "lucide-react"
import { type ReactNode, useState } from "react"
import { rebuildIndex } from "@/api"
import { AddLibraryForm } from "@/components/AddLibraryForm"
import { RemoveLibraryConfirm } from "@/components/RemoveLibraryConfirm"
import { Button } from "@/components/ui/button"
import { IS_TAURI } from "@/lib/platform"
import { useCurrentLibrary, useLibraries } from "@/lib/useAppLibrary"

/** The libraries on this device (app only) and rebuilding the open one's index. */
export function LibrarySection() {
  return (
    <div className="flex flex-col gap-4">
      {IS_TAURI && <Libraries />}
      <RebuildIndex />
    </div>
  )
}

/** App only: the web always shows the server's own library. */
function Libraries() {
  const libraries = useLibraries().data?.libraries ?? []
  const current = useCurrentLibrary()
  const [removing, setRemoving] = useState<string>()
  const [adding, setAdding] = useState(false)

  return (
    <section aria-label="Libraries">
      <h3 className="mb-2 px-1 font-medium">Libraries</h3>
      <div className="divide-y rounded-lg border px-4">
        {libraries.map((library) =>
          removing === library.name ? (
            <div key={library.name} className="py-4">
              <RemoveLibraryConfirm library={library} onCancel={() => setRemoving(undefined)} />
            </div>
          ) : (
            <div key={library.name} className="flex items-center gap-3 py-3">
              <div className="min-w-0 flex-1">
                <p className="flex items-center gap-2 text-sm font-medium">
                  <span className="truncate">{library.name}</span>
                  {library.name === current?.name && (
                    <span className="shrink-0 rounded-full bg-muted px-2 py-0.5 text-xs font-normal text-muted-foreground">
                      Open
                    </span>
                  )}
                </p>
                <p className="truncate text-sm text-muted-foreground">
                  {library.kind === "local" ? "Folder on this device: " : "Shelf server: "}
                  {library.location}
                </p>
              </div>
              <Button
                variant="ghost"
                size="icon"
                className="shrink-0 text-destructive hover:text-destructive"
                onClick={() => setRemoving(library.name)}
                aria-label={`Remove ${library.name}`}
              >
                <CircleMinus />
              </Button>
            </div>
          ),
        )}
        <div className="py-4">
          {adding ? (
            <AddLibraryForm onCancel={() => setAdding(false)} />
          ) : (
            <Button variant="outline" className="w-full" onClick={() => setAdding(true)}>
              <Plus />
              Add library
            </Button>
          )}
        </div>
      </div>
    </section>
  )
}

function RebuildIndex() {
  const queryClient = useQueryClient()
  const rebuild = useMutation({
    mutationFn: rebuildIndex,
    onSuccess: () => {
      // IDs can change and books can disappear, including the one that's open.
      for (const key of ["dirs", "books", "state"]) {
        void queryClient.invalidateQueries({ queryKey: [key] })
      }
    },
  })
  const result = rebuild.data

  return (
    <Row
      title="Rebuild index"
      description="Re-reads every book file and refreshes the library's index. Use it when books are missing, duplicated or out of date. Takes a while on a large library."
      status={
        result &&
        `Done: ${result.books} books, ${result.failed} couldn't be read, ${result.removed} removed.`
      }
      error={rebuild.isError && String(rebuild.error)}
    >
      <Button
        variant="outline"
        className="shrink-0"
        onClick={() => rebuild.mutate()}
        disabled={rebuild.isPending}
      >
        {rebuild.isPending && <LoaderCircle className="animate-spin" />}
        {rebuild.isPending ? "Rebuilding…" : "Rebuild"}
      </Button>
    </Row>
  )
}

function Row({
  title,
  description,
  status,
  error,
  children,
}: {
  title: string
  description?: ReactNode
  status?: ReactNode
  error?: ReactNode
  children: ReactNode
}) {
  return (
    <div className="rounded-lg border px-4">
      <div className="flex flex-col gap-3 py-4 sm:flex-row sm:items-center sm:justify-between sm:gap-6">
        <div className="min-w-0">
          <p className="text-sm font-medium">{title}</p>
          {description && <p className="text-sm text-muted-foreground">{description}</p>}
          {status && <p className="mt-1 text-sm">{status}</p>}
          {error && <p className="mt-1 text-sm text-destructive">{error}</p>}
        </div>
        {children}
      </div>
    </div>
  )
}
