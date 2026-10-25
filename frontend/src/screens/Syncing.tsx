import { LoaderCircle } from "lucide-react"
import { CenteredScreen } from "@/components/CenteredScreen"

export function Syncing() {
  return (
    <CenteredScreen>
      <LoaderCircle className="size-8 animate-spin text-muted-foreground" />
      <h1 className="text-lg font-medium">Syncing library…</h1>
      <p className="max-w-sm text-sm text-muted-foreground">
        Reading new and changed books. The first time can take a while.
      </p>
    </CenteredScreen>
  )
}
