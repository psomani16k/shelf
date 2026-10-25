import type { ListBooksResponseItem } from "@/gen/apipb/list_pb"
import type { BookMetadata } from "@/gen/bookkeeperpb/metadata_pb"

/** The title, or the file name without `.epub` when the book has none. */
export function bookTitle(book: BookMetadata): string {
  return book.title || (book.path.split("/").pop() ?? book.path).replace(/\.epub$/i, "")
}

/** The title, or the file name without `.epub` when the book has none. */
export function itemTitle(item: ListBooksResponseItem): string {
  return item.title || item.fileName.replace(/\.epub$/i, "")
}

export function bookAuthors(book: { authors: string[] }): string {
  return book.authors.join(", ")
}
