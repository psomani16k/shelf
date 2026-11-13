import type { ListBooksResponseItem } from "@/gen/apipb/list_pb"

/** The title, or the file name without `.epub` when the book has none. */
export function itemTitle(item: ListBooksResponseItem): string {
  return item.title || item.fileName.replace(/\.epub$/i, "")
}

export function bookAuthors(book: { authors: string[] }): string {
  return book.authors.join(", ")
}
