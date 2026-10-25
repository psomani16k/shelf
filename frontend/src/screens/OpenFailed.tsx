import { CenteredScreen } from "@/components/CenteredScreen"
import { Button } from "@/components/ui/button"
import { useForgetLibrary } from "@/lib/useAppLibrary"

/** App only: the stored library couldn't be opened. */
export function OpenFailed({ error }: { error: unknown }) {
  const forget = useForgetLibrary()
  return (
    <CenteredScreen>
      <h1 className="text-lg font-medium">Couldn't open the library</h1>
      <p className="max-w-sm text-sm text-destructive">{String(error)}</p>
      <Button onClick={() => forget.mutate()} disabled={forget.isPending}>
        Choose another library
      </Button>
      {forget.isError && <p className="text-sm text-destructive">{String(forget.error)}</p>}
    </CenteredScreen>
  )
}
