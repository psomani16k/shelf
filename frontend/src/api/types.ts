import type { MessageInitShape } from "@bufbuild/protobuf"
import type {
  ListUsersResponse,
  Role_Value,
  SessionInfo,
  User,
  UserRequest,
  UserScopeSchema,
} from "@/gen/apipb/auth_pb"
import type { ListAnnotationsResponse } from "@/gen/apipb/annotation_pb"
import type { ListBooksResponse, ListDirectoriesResponse } from "@/gen/apipb/list_pb"
import type {
  BookCopySchema,
  CleanUpProgressResponse,
  DeleteItemsResponse,
  RebuildIndexResponse,
  UploadEbookResponse,
} from "@/gen/apipb/library_pb"
import type { GetProgressResponse } from "@/gen/apipb/progress_pb"
import type { GetStateResponse } from "@/gen/apipb/state_pb"
import type { ListThemesResponse } from "@/gen/apipb/theme_pb"
import type { ImportUserDataResponse, UserData } from "@/gen/apipb/user_data_pb"
import type { Annotation, HighlightColor_Value } from "@/gen/bookkeeperpb/annotation_pb"
import type { Config, ConfigSchema } from "@/gen/configpb/config_pb"
import type { ReaderThemeSpecSchema } from "@/gen/configpb/theme_pb"

export type NewAnnotation = {
  cfi: string
  text: string
  chapter: string
  note: string
  color: HighlightColor_Value
}

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
  /** The user's highlights in the book, oldest first. */
  listAnnotations: (id: string) => Promise<ListAnnotationsResponse>
  /** Resolves to the highlight as stored, with its new `id` and times. */
  addAnnotation: (id: string, highlight: NewAnnotation) => Promise<Annotation>
  /** Only the fields set change (`note: ""` clears it). Rejects with a 404 for an unknown id. */
  updateAnnotation: (
    id: string,
    annotationId: string,
    patch: { note?: string; color?: HighlightColor_Value },
  ) => Promise<Annotation>
  /** Resolves when it's gone, even if it already was. */
  deleteAnnotation: (id: string, annotationId: string) => Promise<void>
  /** `lastBookId` is unset on first use, or when that book has left the library. */
  getState: () => Promise<GetStateResponse>
  setState: (lastBookId: string) => Promise<void>
  /** The signed-in user's own settings. */
  getConfig: () => Promise<Config>
  /** Only the fields set in `patch` change; resolves to the user's whole config. */
  setConfig: (patch: MessageInitShape<typeof ConfigSchema>) => Promise<Config>
  /** The built-in reader themes with the user's own over them, then the user's other themes. */
  listThemes: () => Promise<ListThemesResponse>
  /** Creates the user's theme, or replaces their one with the same id; resolves to the list. */
  setTheme: (spec: MessageInitShape<typeof ReaderThemeSpecSchema>) => Promise<ListThemesResponse>
  /** Rejects with a 400 for a built-in. Deleting an override brings the built-in back. */
  deleteTheme: (name: string) => Promise<ListThemesResponse>
  /** Also moves the config's choice when it was `from`, so refetch the config. */
  renameTheme: (from: string, to: string) => Promise<ListThemesResponse>
  /** The user's progress, last book, settings and themes, without their name. */
  exportUserData: () => Promise<UserData>
  /**
   * Replaces the user's progress for the books in `data` this library has, their last book and
   * the settings `data` sets. Rejects with a 400 when it isn't a Shelf export.
   */
  importUserData: (data: UserData) => Promise<ImportUserDataResponse>
  /** Re-hashes every book and reloads the index. Takes as long as reading the whole library. */
  rebuildIndex: () => Promise<RebuildIndexResponse>
  /** Deletes every user's progress of books no file has any more. Admins only on a server. */
  cleanUpProgress: () => Promise<CleanUpProgressResponse>
  /** The thumbnail JPEG. Rejects with a 404 `ApiError` when the book has no cover. */
  getCover: (id: string) => Promise<Blob>
  /** The EPUB file, ready for foliate-js. */
  getBookFile: (id: string) => Promise<Blob>
  /**
   * Adds `file` to the folder `directory` (`""` is the root). `onProgress` gets the bytes sent so
   * far. Rejects with a 409 when the name is taken or the book is already in the library, and a
   * 400 when it isn't a readable EPUB.
   */
  uploadBook: (
    directory: string,
    file: File,
    onProgress?: (sentBytes: number) => void,
  ) => Promise<UploadEbookResponse>
  /** Rejects with a 409 when something in `parent` already has the name. */
  createDirectory: (parent: string, name: string) => Promise<void>
  /** Folders go with everything in them. Unknown book ids are skipped. */
  deleteItems: (items: DeleteItems) => Promise<DeleteItemsResponse>
  /**
   * Signs in to the server (the app: the open remote library) and keeps the key on this device.
   * Rejects with a 401 for a wrong password and a 429 after too many.
   */
  login: (username: string, password: string) => Promise<User>
  /** Forgets this device's key, and ends its session on the server when it can be reached. */
  logout: () => Promise<void>
  /** The signed-in user; `null` for an app's local library, which has no accounts. */
  me: () => Promise<User | null>
  /** The user's sessions on the server, this one first. Not for a local library. */
  listSessions: () => Promise<SessionInfo[]>
  /**
   * Signs out another of the user's devices, by `SessionInfo.id`, and gives the sessions left.
   * Rejects with a 400 for this device's own session (that's `logout`).
   */
  signOutSession: (id: string) => Promise<SessionInfo[]>
  /** Signs out every other device. Rejects with a 400 when `currentPassword` is wrong. */
  changePassword: (currentPassword: string, newPassword: string) => Promise<void>
  /**
   * Asks the server's admins for an account; needs no sign-in. Rejects with a 409 when a user
   * already has the name, a 400 for a bad name or password, and a 429 when too many are waiting.
   */
  signUp: (username: string, password: string) => Promise<void>
  /** Pending sign-ups, oldest first. Admins and owners only. */
  listUserRequests: () => Promise<UserRequest[]>
  /**
   * Makes the request a user with `role`. Rejects with a 409 when the name has been taken since,
   * and a 403 for a role the caller can't give.
   */
  approveUserRequest: (id: string, role: Role_Value) => Promise<void>
  denyUserRequest: (id: string) => Promise<void>
  /** Every user, by name. Admins and owners only. */
  listUsers: () => Promise<ListUsersResponse>
  /**
   * Rejects with a 403 for the caller's own role, or a role they can't give or take away, and a
   * 404 for an unknown user. Banning also signs the user out everywhere.
   */
  setUserRole: (username: string, role: Role_Value) => Promise<void>
  /**
   * A user's library root and book globs, from their next request. Same rule as `setUserRole`.
   * Rejects with a 400 for a bad pattern or a root that isn't a folder.
   */
  setUserScope: (username: string, scope: MessageInitShape<typeof UserScopeSchema>) => Promise<void>
  /** Subfolders of a folder in the whole library, whatever the caller's own scope. Admins only. */
  listScopeDirectories: (directoryPath: string) => Promise<string[]>
}

/** Paths are relative to the library root. */
export type DeleteItems = {
  /** One exact copy each. */
  books: MessageInitShape<typeof BookCopySchema>[]
  directoryPaths: string[]
}
