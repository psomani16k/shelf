import type { MessageInitShape } from "@bufbuild/protobuf"
import type { ListBooksResponse, ListDirectoriesResponse } from "@/gen/apipb/list_pb"
import type { GetProgressResponse } from "@/gen/apipb/progress_pb"
import type { GetStateResponse } from "@/gen/apipb/state_pb"
import type { Config, ConfigSchema } from "@/gen/configpb/config_pb"

/**
 * Everything the UI needs from a library. The server implements it over HTTP, the app over
 * Tauri commands; both speak the same proto messages.
 */
export type Backend = {
  /** `directoryPath` is relative to the library root; `""` is the root. */
  listBooks: (directoryPath: string) => Promise<ListBooksResponse>
  /** `directoryPath` is relative to the library root; `""` is the root. */
  listDirectories: (directoryPath: string) => Promise<ListDirectoriesResponse>
  /** `epubProgress` is unset when the book hasn't been read yet. */
  getProgress: (id: string) => Promise<GetProgressResponse>
  /** `epubProgress` is an EPUB CFI. `keepalive` lets the request finish after the tab closes. */
  setProgress: (id: string, epubProgress: string, options?: { keepalive?: boolean }) => Promise<void>
  /** `lastBookId` is unset on first use, or when that book has left the library. */
  getState: () => Promise<GetStateResponse>
  setState: (lastBookId: string) => Promise<void>
  getConfig: () => Promise<Config>
  /** Only the fields set in `patch` change; resolves to the whole config. */
  setConfig: (patch: MessageInitShape<typeof ConfigSchema>) => Promise<Config>
  /** The thumbnail JPEG. Rejects with a 404 `ApiError` when the book has no cover. */
  getCover: (id: string) => Promise<Blob>
  /** The EPUB file, ready for foliate-js. */
  getBookFile: (id: string) => Promise<Blob>
}
