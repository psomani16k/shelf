import type { LibraryInfo } from "@/api/app"
import { InlineConfirm } from "@/components/InlineConfirm"
import { useRemoveLibrary } from "@/lib/useAppLibrary"

/** App only: asks before forgetting a library, and says that nothing in it is deleted. */
export function RemoveLibraryConfirm({
  library,
  onCancel,
}: {
  library: LibraryInfo
  onCancel: () => void
}) {
  const remove = useRemoveLibrary()
  const where = library.kind === "local" ? "the folder" : "the server"

  return (
    <InlineConfirm
      className="text-left"
      confirmLabel="Remove"
      onConfirm={() => remove.mutate(library.name)}
      onCancel={onCancel}
      pending={remove.isPending}
      error={remove.isError && String(remove.error)}
    >
      <span className="font-medium">Remove {library.name} from this device?</span> Shelf only
      forgets it here. Nothing is deleted: its books, reading progress and settings stay in{" "}
      {where}, and you can add it again any time.
    </InlineConfirm>
  )
}
