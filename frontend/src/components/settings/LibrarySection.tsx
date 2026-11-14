import { fromJsonString, toJsonString } from "@bufbuild/protobuf"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { CircleMinus, LoaderCircle, Plus } from "lucide-react"
import { type ReactNode, useRef, useState } from "react"
import { cleanUpProgress, exportUserData, importUserData, rebuildIndex } from "@/api"
import { saveJsonFile } from "@/api/app"
import { AddLibraryForm } from "@/components/AddLibraryForm"
import { RemoveLibraryConfirm } from "@/components/RemoveLibraryConfirm"
import { Button } from "@/components/ui/button"
import { type UserData, UserDataSchema } from "@/gen/apipb/user_data_pb"
import { IS_TAURI } from "@/lib/platform"
import { useCurrentLibrary, useLibraries } from "@/lib/useAppLibrary"
import { usePermissions } from "@/lib/useMe"

/**
 * The libraries on this device (app only), moving the user's data in and out of the open one,
 * and rebuilding and cleaning it up.
 */
export function LibrarySection() {
  const { canAdmin } = usePermissions()
  return (
    <div className="flex flex-col gap-4">
      {IS_TAURI && <Libraries />}
      <ExportData />
      <ImportData />
      {canAdmin && (
        <>
          <RebuildIndex />
          <CleanUpProgress />
        </>
      )}
      {!canAdmin && !IS_TAURI && (
        <p className="text-sm text-muted-foreground">
          Rebuilding the index and cleaning up progress need an admin.
        </p>
      )}
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

/** The `UserData.version` this build writes and reads. */
const USER_DATA_VERSION = 1

const books = (n: number) => `${n} book${n === 1 ? "" : "s"}`

function ExportData() {
  const save = useMutation({
    mutationFn: async () => {
      const data = await exportUserData()
      const contents = toJsonString(UserDataSchema, data, {
        useProtoFieldName: true,
        prettySpaces: 2,
      })
      const name = `shelf-data-${new Date().toISOString().slice(0, 10)}.json`
      const saved = IS_TAURI ? await saveJsonFile(name, contents) : download(name, contents)
      return { saved, books: data.progress.length }
    },
  })
  const result = save.data

  return (
    <Row
      title="Export your reading data"
      description="Saves your reading progress, last book and settings as a file, to import into another library: a server, or a folder on a device."
      status={result?.saved && `Exported your progress for ${books(result.books)}.`}
      error={save.isError && String(save.error)}
    >
      <Button
        variant="outline"
        className="shrink-0"
        onClick={() => save.mutate()}
        disabled={save.isPending}
      >
        {save.isPending && <LoaderCircle className="animate-spin" />}
        Export
      </Button>
    </Row>
  )
}

/** Web only: the browser saves it to its downloads. */
function download(name: string, contents: string): true {
  const url = URL.createObjectURL(new Blob([contents], { type: "application/json" }))
  const link = document.createElement("a")
  link.href = url
  link.download = name
  link.click()
  setTimeout(() => URL.revokeObjectURL(url), 10_000)
  return true
}

function ImportData() {
  const queryClient = useQueryClient()
  const fileInput = useRef<HTMLInputElement>(null)
  const [picked, setPicked] = useState<UserData>()
  const [badFile, setBadFile] = useState(false)
  const run = useMutation({
    mutationFn: importUserData,
    // Progress, the last book and settings all may have changed.
    onSuccess: () => void queryClient.invalidateQueries(),
    onSettled: () => setPicked(undefined),
  })
  const result = run.data

  const pick = async (file: File | undefined) => {
    if (!file) return
    run.reset()
    setBadFile(false)
    try {
      const data = fromJsonString(UserDataSchema, await file.text(), { ignoreUnknownFields: true })
      if (data.version !== USER_DATA_VERSION) throw new Error("unknown version")
      setPicked(data)
    } catch {
      setBadFile(true)
    }
  }

  return (
    <Row
      title="Import reading data"
      description={
        picked
          ? `Import your progress for ${books(picked.progress.length)}? It replaces your progress for those books, your last book and your settings.`
          : "Loads a file exported from Shelf, from this library or another one, as your own data."
      }
      status={
        result &&
        `Imported your progress for ${books(result.books)}` +
          (result.skipped > 0 ? `; ${result.skipped} aren't in this library.` : ".")
      }
      error={
        (badFile && "This isn't a Shelf data export.") || (run.isError && String(run.error))
      }
    >
      {picked ? (
        <div className="flex shrink-0 gap-2">
          <Button variant="ghost" onClick={() => setPicked(undefined)} disabled={run.isPending}>
            Cancel
          </Button>
          <Button onClick={() => run.mutate(picked)} disabled={run.isPending}>
            {run.isPending && <LoaderCircle className="animate-spin" />}
            Import
          </Button>
        </div>
      ) : (
        <Button variant="outline" className="shrink-0" onClick={() => fileInput.current?.click()}>
          Choose file
        </Button>
      )}
      <input
        ref={fileInput}
        type="file"
        accept=".json,application/json"
        className="hidden"
        onChange={(e) => {
          void pick(e.target.files?.[0])
          // So picking the same file again still fires `change`.
          e.target.value = ""
        }}
      />
    </Row>
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

function CleanUpProgress() {
  const [confirming, setConfirming] = useState(false)
  const clean = useMutation({ mutationFn: cleanUpProgress, onSettled: () => setConfirming(false) })
  const removed = clean.data?.books

  return (
    <Row
      title="Clean up progress"
      description={
        confirming
          ? "Delete it? This can't be undone."
          : "Deletes the saved reading progress, for every user, of books that are no longer in the library. A book added back afterwards starts from the beginning."
      }
      status={
        removed !== undefined && `Removed the progress of ${removed} book${removed === 1 ? "" : "s"}.`
      }
      error={clean.isError && String(clean.error)}
    >
      {confirming ? (
        <div className="flex shrink-0 gap-2">
          <Button variant="ghost" onClick={() => setConfirming(false)} disabled={clean.isPending}>
            Cancel
          </Button>
          <Button variant="destructive" onClick={() => clean.mutate()} disabled={clean.isPending}>
            {clean.isPending && <LoaderCircle className="animate-spin" />}
            Delete
          </Button>
        </div>
      ) : (
        <Button
          variant="outline"
          className="shrink-0"
          onClick={() => {
            clean.reset()
            setConfirming(true)
          }}
        >
          Clean up
        </Button>
      )}
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
