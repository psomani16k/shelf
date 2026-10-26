import type { LibraryInfo } from "@/api/app"
import { Button } from "@/components/ui/button"
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
    <div className="flex flex-col gap-3 text-left">
      <p className="text-sm">
        <span className="font-medium">Remove {library.name} from this device?</span> Shelf only
        forgets it here. Nothing is deleted: its books, reading progress and settings stay in{" "}
        {where}, and you can add it again any time.
      </p>
      {remove.isError && <p className="text-sm text-destructive">{String(remove.error)}</p>}
      <div className="flex justify-end gap-2">
        <Button variant="ghost" onClick={onCancel} disabled={remove.isPending}>
          Cancel
        </Button>
        <Button
          variant="destructive"
          onClick={() => remove.mutate(library.name)}
          disabled={remove.isPending}
        >
          Remove
        </Button>
      </div>
    </div>
  )
}
