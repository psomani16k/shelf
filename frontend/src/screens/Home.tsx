import { create } from "@bufbuild/protobuf"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import type { TocItem } from "foliate-js/view.js"
import { LibraryBig, LoaderCircle, PanelLeft, TableOfContents } from "lucide-react"
import { useRef, useState } from "react"
import { Route, Routes, useMatch, useNavigate } from "react-router"
import { getState, setState } from "@/api"
import { LibraryPanel } from "@/components/LibraryPanel"
import { Reader, type ReaderHandle } from "@/components/Reader"
import { ScreenHeader } from "@/components/ScreenHeader"
import { type DrawerSide, SlideDrawer } from "@/components/SlideDrawer"
import { TocPanel } from "@/components/TocPanel"
import { Button } from "@/components/ui/button"
import { GetStateResponseSchema } from "@/gen/apipb/state_pb"
import { useAppearance } from "@/lib/useAppearance"
import { Settings, SETTINGS_PATH } from "@/screens/Settings"

/** Opens the last book read, or nudges towards the library when there isn't one. */
export function Home() {
  const queryClient = useQueryClient()
  const [drawer, setDrawer] = useState<DrawerSide | null>(null)
  // Kept only in memory: the library reopens where it was left until the tab closes or reloads.
  const [folder, setFolder] = useState("")
  const [title, setTitle] = useState("")
  const [toc, setToc] = useState<TocItem[]>([])
  const [tocId, setTocId] = useState<number>()
  const readerRef = useRef<ReaderHandle>(null)
  const navigate = useNavigate()
  const settingsOpen = useMatch(`${SETTINGS_PATH}/*`) !== null
  const appearance = useAppearance()

  const state = useQuery({
    queryKey: ["state"],
    queryFn: getState,
    staleTime: Infinity,
    refetchOnWindowFocus: false,
  })
  const pick = useMutation({
    mutationFn: setState,
    onSuccess: (_, id) => {
      if (id !== bookId) {
        setTitle("")
        setToc([])
        setTocId(undefined)
      }
      queryClient.setQueryData(["state"], create(GetStateResponseSchema, { lastBookId: id }))
      setDrawer(null)
    },
  })
  const bookId = state.data?.lastBookId
  const openLibrary = () => setDrawer("left")
  const close = () => setDrawer(null)

  const header = (
    <ScreenHeader
      title={title || "Shelf"}
      start={
        <Button variant="ghost" size="icon" onClick={openLibrary} aria-label="Open library">
          <PanelLeft />
        </Button>
      }
      end={
        bookId && (
          <Button
            variant="ghost"
            size="icon"
            onClick={() => setDrawer("right")}
            aria-label="Open contents"
          >
            <TableOfContents />
          </Button>
        )
      }
    />
  )

  return (
    <>
      {/* Settings covers the page; the reader and drawers stay mounted behind it. */}
      <div inert={settingsOpen}>
        <SlideDrawer
          open={drawer}
          onOpenChange={setDrawer}
          header={header}
          left={{
            label: "Library",
            content: (
              <LibraryPanel
                folder={folder}
                onFolderChange={setFolder}
                onPick={(id) => pick.mutate(id)}
                pickError={pick.error}
                onClose={close}
                onOpenSettings={() => void navigate(SETTINGS_PATH, { state: { depth: 1 } })}
              />
            ),
          }}
          right={
            bookId
              ? {
                  label: "Contents",
                  content: (
                    <TocPanel
                      key={bookId}
                      toc={toc}
                      currentId={tocId}
                      open={drawer === "right"}
                      onPick={(href) => {
                        readerRef.current?.goTo(href)
                        close()
                      }}
                      onClose={close}
                    />
                  ),
                }
              : undefined
          }
        >
          {state.isPending ? (
            <LoaderCircle className="absolute inset-0 m-auto size-8 animate-spin text-muted-foreground" />
          ) : state.isError ? (
            <div className="flex h-full flex-col items-center justify-center gap-4 p-6 text-center">
              <p className="text-destructive">{String(state.error)}</p>
              <Button onClick={() => void state.refetch()}>Try again</Button>
            </div>
          ) : bookId ? (
            <Reader
              key={bookId}
              ref={readerRef}
              id={bookId}
              paused={drawer !== null || settingsOpen}
              appearance={appearance.reader}
              onOpen={(book) => {
                setTitle(book.title)
                setToc(book.toc)
              }}
              onLocation={setTocId}
              onOpenLibrary={openLibrary}
            />
          ) : (
            <div className="flex h-full flex-col items-center justify-center gap-4 p-6 text-center">
              <LibraryBig className="size-10 text-muted-foreground" />
              <div>
                <h2 className="text-lg font-medium">Pick a book to start reading</h2>
                <p className="text-sm text-muted-foreground">Your books are in the library.</p>
              </div>
              <Button onClick={openLibrary}>Open library</Button>
            </div>
          )}
        </SlideDrawer>
      </div>
      <Routes>
        <Route path={`${SETTINGS_PATH}/:section?`} element={<Settings />} />
        {/* Every other path is the reader, which is always rendered above. */}
        <Route path="*" element={null} />
      </Routes>
    </>
  )
}
