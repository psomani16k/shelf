import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
  Circle,
  CircleCheck,
  Folder,
  FolderPlus,
  ListChecks,
  LoaderCircle,
  Settings,
  Trash2,
  Upload,
  X,
} from "lucide-react"
import { type ReactNode, useRef, useState } from "react"
import { createDirectory, deleteItems, listBooks, listDirectories } from "@/api"
import { BookCover } from "@/components/BookCover"
import { Breadcrumb } from "@/components/Breadcrumb"
import { LibrarySwitcher } from "@/components/LibrarySwitcher"
import { UploadList } from "@/components/UploadList"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import type { ListBooksResponseItem } from "@/gen/apipb/list_pb"
import { bookAuthors, itemTitle } from "@/lib/book"
import { IS_TAURI } from "@/lib/platform"
import { uploads } from "@/lib/uploads"
import { useLongPress } from "@/lib/useLongPress"
import { usePermissions } from "@/lib/useMe"
import { cn } from "cn"

type Props = {
  /** Relative to the library root; `""` is the root. */
  folder: string
  onFolderChange: (folder: string) => void
  onPick: (id: string) => void
  pickError?: unknown
  onClose: () => void
  onOpenSettings: () => void
}

/** What's ticked in the open folder: book file names and subfolder names. */
type Selection = { books: Set<string>; dirs: Set<string> }

/** The bottom bar's other forms, in place of its buttons. */
type BarForm = "new-folder" | "confirm-delete"

export function LibraryPanel({
  folder,
  onFolderChange,
  onPick,
  pickError,
  onClose,
  onOpenSettings,
}: Props) {
  const queryClient = useQueryClient()
  const dirs = useQuery({ queryKey: ["dirs", folder], queryFn: () => listDirectories(folder) })
  const books = useQuery({ queryKey: ["books", folder], queryFn: () => listBooks(folder) })
  const [selection, setSelection] = useState<Selection | null>(null)
  const [form, setForm] = useState<BarForm | null>(null)
  const fileInput = useRef<HTMLInputElement>(null)

  const error = dirs.error ?? books.error
  const subDirs = dirs.data?.subDirectories ?? []
  const bookItems = books.data?.books ?? []
  const empty = dirs.data && books.data && subDirs.length === 0 && bookItems.length === 0
  const pathOf = (name: string) => (folder ? `${folder}/${name}` : name)

  const goTo = (next: string) => {
    setSelection(null)
    setForm(null)
    onFolderChange(next)
  }

  // Selecting is only for deleting, which needs an editor.
  const { canEdit } = usePermissions()
  const startSelecting = (first?: { book?: string; dir?: string }) => {
    if (!canEdit) return
    setForm(null)
    setSelection({
      books: new Set(first?.book ? [first.book] : []),
      dirs: new Set(first?.dir ? [first.dir] : []),
    })
  }
  const toggle = (kind: keyof Selection, key: string) =>
    setSelection((s) => {
      if (!s) return s
      const next = new Set(s[kind])
      if (!next.delete(key)) next.add(key)
      return { ...s, [kind]: next }
    })
  const count = selection ? selection.books.size + selection.dirs.size : 0
  const allSelected = count === subDirs.length + bookItems.length
  const selectAll = () =>
    setSelection(
      allSelected
        ? { books: new Set(), dirs: new Set() }
        : { books: new Set(bookItems.map((b) => b.fileName)), dirs: new Set(subDirs) },
    )

  const remove = useMutation({
    mutationFn: (s: Selection) =>
      deleteItems({
        books: bookItems
          .filter((b) => s.books.has(b.fileName))
          .map((b) => ({ id: b.id, directoryPath: folder, fileName: b.fileName })),
        directoryPaths: [...s.dirs].map(pathOf),
      }),
    onSuccess: () => {
      setSelection(null)
      setForm(null)
    },
    // Some may be gone even when one failed. The open book may be gone too, which `state` shows.
    onSettled: () =>
      Promise.all(
        [["books"], ["dirs"], ["state"]].map((queryKey) => queryClient.invalidateQueries({ queryKey })),
      ),
  })

  const upload = (files: FileList | null) => {
    if (!files?.length) return
    const into = folder
    uploads.enqueue(into, [...files], () => {
      void queryClient.invalidateQueries({ queryKey: ["books", into] })
    })
  }

  return (
    <>
      <div className="flex items-start gap-2 border-b py-3 pr-2 pl-4">
        <div className="min-w-0 flex-1">
          <h2 className="font-medium">Library</h2>
          <Breadcrumb folder={folder} onNavigate={goTo} />
        </div>
        <Button variant="ghost" size="icon" onClick={onClose} aria-label="Close library">
          <X />
        </Button>
      </div>

      <div className="flex-1 overflow-y-auto overscroll-contain p-4">
        {(dirs.isPending || books.isPending) && !error && (
          <LoaderCircle className="mx-auto mt-8 size-6 animate-spin text-muted-foreground" />
        )}
        {!!error && <p className="text-destructive">{String(error)}</p>}
        {!!pickError && <p className="mb-4 text-destructive">{String(pickError)}</p>}
        {empty && <p className="mt-8 text-center text-muted-foreground">No books here.</p>}
        {dirs.data && books.data && (
          <div className="grid grid-cols-[repeat(auto-fill,minmax(8rem,1fr))] gap-4">
            {subDirs.map((name) => (
              <FolderTile
                key={name}
                name={name}
                selected={selection && selection.dirs.has(name)}
                onClick={() => (selection ? toggle("dirs", name) : goTo(pathOf(name)))}
                onLongPress={() => !selection && startSelecting({ dir: name })}
              />
            ))}
            {bookItems.map((book) => (
              <BookTile
                key={book.fileName}
                book={book}
                selected={selection && selection.books.has(book.fileName)}
                onClick={() => (selection ? toggle("books", book.fileName) : onPick(book.id))}
                onLongPress={() => !selection && startSelecting({ book: book.fileName })}
              />
            ))}
          </div>
        )}
      </div>

      <UploadList />
      {IS_TAURI && <LibrarySwitcher side="top" className="border-t p-2" />}
      <div className="border-t p-2">
        {form === "new-folder" ? (
          <NewFolderForm folder={folder} onDone={() => setForm(null)} />
        ) : form === "confirm-delete" && selection ? (
          <div className="flex flex-col gap-3 p-2">
            <p className="text-sm">
              <span className="font-medium">Delete {describe(selection)}?</span>
              {selection.dirs.size > 0 && " Folders are deleted with everything in them."} This
              can't be undone.
            </p>
            {remove.isError && <p className="text-sm text-destructive">{String(remove.error)}</p>}
            <div className="flex justify-end gap-2">
              <Button variant="ghost" onClick={() => setForm(null)} disabled={remove.isPending}>
                Cancel
              </Button>
              <Button
                variant="destructive"
                onClick={() => remove.mutate(selection)}
                disabled={remove.isPending}
              >
                Delete
              </Button>
            </div>
          </div>
        ) : selection ? (
          <div className="flex items-center gap-1">
            <Button
              variant="ghost"
              size="icon"
              onClick={() => setSelection(null)}
              aria-label="Stop selecting"
            >
              <X />
            </Button>
            <span className="flex-1 text-sm">{count} selected</span>
            <Button variant="ghost" size="sm" onClick={selectAll}>
              {allSelected ? "Select none" : "Select all"}
            </Button>
            <Button
              variant="ghost"
              size="icon"
              className="text-destructive"
              onClick={() => {
                remove.reset()
                setForm("confirm-delete")
              }}
              disabled={count === 0}
              aria-label="Delete selected"
            >
              <Trash2 />
            </Button>
          </div>
        ) : (
          <div className="flex items-center gap-1">
            <Button variant="ghost" size="icon" onClick={onOpenSettings} aria-label="Settings">
              <Settings />
            </Button>
            <div className="flex-1" />
            {canEdit && (
              <>
                <Button
                  variant="ghost"
                  size="icon"
                  onClick={() => startSelecting()}
                  disabled={!!empty || !dirs.data || !books.data}
                  aria-label="Select"
                >
                  <ListChecks />
                </Button>
                <Button
                  variant="ghost"
                  size="icon"
                  onClick={() => setForm("new-folder")}
                  aria-label="New folder"
                >
                  <FolderPlus />
                </Button>
                <Button
                  variant="ghost"
                  size="icon"
                  onClick={() => fileInput.current?.click()}
                  aria-label="Upload books"
                >
                  <Upload />
                </Button>
                <input
                  ref={fileInput}
                  type="file"
                  multiple
                  accept=".epub,application/epub+zip"
                  className="hidden"
                  onChange={(e) => {
                    upload(e.target.files)
                    // So picking the same file again still fires `change`.
                    e.target.value = ""
                  }}
                />
              </>
            )}
          </div>
        )}
      </div>
    </>
  )
}

/** "2 books and 1 folder" */
function describe({ books, dirs }: Selection): string {
  const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`
  const parts = [
    books.size > 0 && plural(books.size, "book"),
    dirs.size > 0 && plural(dirs.size, "folder"),
  ].filter(Boolean)
  return parts.join(" and ")
}

function NewFolderForm({ folder, onDone }: { folder: string; onDone: () => void }) {
  const queryClient = useQueryClient()
  const [name, setName] = useState("")
  const create = useMutation({
    mutationFn: () => createDirectory(folder, name.trim()),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["dirs", folder] })
      onDone()
    },
  })

  return (
    <form
      className="flex flex-col gap-2 p-2"
      onSubmit={(e) => {
        e.preventDefault()
        if (name.trim()) create.mutate()
      }}
    >
      <Input
        autoFocus
        value={name}
        onChange={(e) => setName(e.target.value)}
        placeholder="New folder name"
        aria-label="New folder name"
        disabled={create.isPending}
      />
      {create.isError && <p className="text-sm text-destructive">{String(create.error)}</p>}
      <div className="flex justify-end gap-2">
        <Button type="button" variant="ghost" onClick={onDone} disabled={create.isPending}>
          Cancel
        </Button>
        <Button type="submit" disabled={!name.trim() || create.isPending}>
          Create
        </Button>
      </div>
    </form>
  )
}

type TileProps = {
  /** `null` when not selecting. */
  selected: boolean | null
  onClick: () => void
  onLongPress: () => void
}

/** A tile's button, with the check mark it shows while selecting. */
function Tile({
  selected,
  onClick,
  onLongPress,
  className,
  children,
}: TileProps & { className?: string; children: ReactNode }) {
  const longPress = useLongPress(onLongPress)
  return (
    <button
      type="button"
      onClick={onClick}
      {...longPress}
      aria-pressed={selected ?? undefined}
      className={cn(
        "relative flex flex-col gap-2 text-left select-none [-webkit-touch-callout:none]",
        className,
      )}
    >
      {children}
      {selected !== null && (
        <span className="absolute top-1.5 right-1.5 rounded-full bg-background/80 text-primary">
          {selected ? <CircleCheck className="size-6" /> : <Circle className="size-6" />}
        </span>
      )}
    </button>
  )
}

function FolderTile({ name, ...tile }: TileProps & { name: string }) {
  return (
    <Tile {...tile}>
      <div
        className={cn(
          "flex aspect-[2/3] items-center justify-center rounded-md bg-muted/50 text-muted-foreground transition-colors hover:bg-muted",
          tile.selected && "ring-2 ring-primary",
        )}
      >
        <Folder className="size-1/3" />
      </div>
      <span className="line-clamp-2 text-sm font-medium">{name}</span>
    </Tile>
  )
}

function BookTile({ book, ...tile }: TileProps & { book: ListBooksResponseItem }) {
  return (
    <Tile {...tile} className="group">
      <BookCover
        id={book.id}
        className={cn(
          "transition-opacity group-hover:opacity-80",
          tile.selected && "ring-2 ring-primary",
        )}
      />
      <div className="min-w-0">
        <p className="line-clamp-2 text-sm font-medium">{itemTitle(book)}</p>
        <p className="truncate text-xs text-muted-foreground">{bookAuthors(book)}</p>
      </div>
    </Tile>
  )
}
