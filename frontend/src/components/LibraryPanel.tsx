import { useQuery } from "@tanstack/react-query"
import { ChevronRight, Folder, LoaderCircle, Settings, X } from "lucide-react"
import { listBooks, listDirectories } from "@/api"
import { BookCover } from "@/components/BookCover"
import { Button } from "@/components/ui/button"
import type { ListBooksResponseItem } from "@/gen/apipb/list_pb"
import { bookAuthors, itemTitle } from "@/lib/book"

type Props = {
  /** Relative to the library root; `""` is the root. */
  folder: string
  onFolderChange: (folder: string) => void
  onPick: (id: string) => void
  pickError?: unknown
  onClose: () => void
  onOpenSettings: () => void
}

export function LibraryPanel({
  folder,
  onFolderChange,
  onPick,
  pickError,
  onClose,
  onOpenSettings,
}: Props) {
  const dirs = useQuery({ queryKey: ["dirs", folder], queryFn: () => listDirectories(folder) })
  const books = useQuery({ queryKey: ["books", folder], queryFn: () => listBooks(folder) })

  const error = dirs.error ?? books.error
  const empty = dirs.data?.subDirectories.length === 0 && books.data?.books.length === 0

  return (
    <>
      <div className="flex items-start gap-2 border-b py-3 pr-2 pl-4">
        <div className="min-w-0 flex-1">
          <h2 className="font-medium">Library</h2>
          <Breadcrumb folder={folder} onNavigate={onFolderChange} />
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
            {dirs.data.subDirectories.map((name) => (
              <FolderTile
                key={name}
                name={name}
                onClick={() => onFolderChange(folder ? `${folder}/${name}` : name)}
              />
            ))}
            {books.data.books.map((book) => (
              <BookTile key={book.id} book={book} onClick={() => onPick(book.id)} />
            ))}
          </div>
        )}
      </div>

      <div className="flex items-center gap-1 border-t p-2">
        <Button variant="ghost" size="icon" onClick={onOpenSettings} aria-label="Settings">
          <Settings />
        </Button>
      </div>
    </>
  )
}

function Breadcrumb({ folder, onNavigate }: { folder: string; onNavigate: (f: string) => void }) {
  const parts = folder ? folder.split("/") : []
  return (
    <nav className="mt-1 flex flex-wrap items-center gap-1 text-sm text-muted-foreground">
      <button type="button" className="hover:text-foreground" onClick={() => onNavigate("")}>
        All books
      </button>
      {parts.map((part, i) => (
        <span key={i} className="flex items-center gap-1">
          <ChevronRight className="size-3.5" />
          <button
            type="button"
            className="hover:text-foreground"
            onClick={() => onNavigate(parts.slice(0, i + 1).join("/"))}
          >
            {part}
          </button>
        </span>
      ))}
    </nav>
  )
}

function FolderTile({ name, onClick }: { name: string; onClick: () => void }) {
  return (
    <button type="button" onClick={onClick} className="flex flex-col gap-2 text-left">
      <div className="flex aspect-[2/3] items-center justify-center rounded-md bg-muted/50 text-muted-foreground transition-colors hover:bg-muted">
        <Folder className="size-1/3" />
      </div>
      <span className="line-clamp-2 text-sm font-medium">{name}</span>
    </button>
  )
}

function BookTile({ book, onClick }: { book: ListBooksResponseItem; onClick: () => void }) {
  return (
    <button type="button" onClick={onClick} className="group flex flex-col gap-2 text-left">
      <BookCover id={book.id} className="transition-opacity group-hover:opacity-80" />
      <div className="min-w-0">
        <p className="line-clamp-2 text-sm font-medium">{itemTitle(book)}</p>
        <p className="truncate text-xs text-muted-foreground">{bookAuthors(book)}</p>
      </div>
    </button>
  )
}
