import { LibraryBig } from "lucide-react"
import { AddLibraryForm } from "@/components/AddLibraryForm"
import { CenteredScreen } from "@/components/CenteredScreen"

/** App only: shown until this device has a library to open. */
export function PickLibrary() {
  return (
    <CenteredScreen>
      <LibraryBig className="size-10 text-muted-foreground" />
      <h1 className="text-2xl font-semibold">Welcome to Shelf</h1>
      <p className="max-w-sm text-muted-foreground">
        Your books can live in a folder on this device, or on a Shelf server. Shelf keeps its own
        data in a hidden .shelf folder next to them.
      </p>
      <div className="w-full max-w-sm rounded-lg border p-4">
        <AddLibraryForm />
      </div>
    </CenteredScreen>
  )
}
