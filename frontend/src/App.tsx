import { useQuery, useQueryClient } from "@tanstack/react-query"
import { LoaderCircle } from "lucide-react"
import { useEffect } from "react"
import { BrowserRouter, MemoryRouter } from "react-router"
import { onLibraryChanged, openLibrary } from "@/api/app"
import { CenteredScreen } from "@/components/CenteredScreen"
import { IS_TAURI } from "@/lib/platform"
import { useLibraries } from "@/lib/useAppLibrary"
import { useSystemInsets } from "@/lib/useSystemInsets"
import { Home } from "@/screens/Home"
import { OpenFailed } from "@/screens/OpenFailed"
import { PickLibrary } from "@/screens/PickLibrary"
import { Syncing } from "@/screens/Syncing"

export default function App() {
  return IS_TAURI ? (
    <AppFlow />
  ) : (
    <BrowserRouter>
      <Home />
    </BrowserRouter>
  )
}

/** The app has to open a library before it can show the same UI as the web. */
function AppFlow() {
  const queryClient = useQueryClient()
  useSystemInsets()
  // The watcher added, changed or removed a book.
  useEffect(
    () =>
      onLibraryChanged(() => {
        void queryClient.invalidateQueries({ queryKey: ["dirs"] })
        void queryClient.invalidateQueries({ queryKey: ["books"] })
      }),
    [queryClient],
  )

  const libraries = useLibraries()
  // One library for now; the Rust side already stores a list.
  const current = libraries.data?.[0]
  const open = useQuery({
    queryKey: ["open-library", current?.kind, current?.location],
    queryFn: () => openLibrary(0),
    enabled: !!current,
    staleTime: Infinity,
    gcTime: Infinity,
    retry: false,
    refetchOnWindowFocus: false,
  })

  if (libraries.isPending) {
    return (
      <CenteredScreen>
        <LoaderCircle className="size-8 animate-spin text-muted-foreground" />
      </CenteredScreen>
    )
  }
  if (libraries.isError) return <OpenFailed error={libraries.error} />
  if (!current) return <PickLibrary />
  if (open.isPending) return <Syncing />
  if (open.isError) return <OpenFailed error={open.error} />
  return (
    // In memory, so each library starts at "/" and no URL survives a change of library.
    <MemoryRouter>
      <Home />
    </MemoryRouter>
  )
}
