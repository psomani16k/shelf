import { listen } from "@tauri-apps/api/event"
import { invokeCommand } from "./tauri"

/** App only: the libraries this device knows about, kept in the app's private storage. */

export type LibraryInfo = {
  name: string
  kind: "local" | "remote"
  /** The folder for a local library, the server URL for a remote one. */
  location: string
}

export const listLibraries = () => invokeCommand<LibraryInfo[]>("list_libraries")

export const addLocalLibrary = (path: string) =>
  invokeCommand<LibraryInfo>("add_local_library", { path })

/** `baseUrl` is the server's origin, e.g. `http://nas:8080`; `http://` is assumed when left out. */
export const addRemoteLibrary = (baseUrl: string) =>
  invokeCommand<LibraryInfo>("add_remote_library", { baseUrl })

export const removeLibrary = (index: number) =>
  invokeCommand<void>("remove_library", { index })

/** Resolves once the library is ready to browse; a local one syncs its `.shelf/` first. */
export const openLibrary = (index: number) => invokeCommand<void>("open_library", { index })

/** The watcher synced a book. Returns the unsubscribe function. */
export function onLibraryChanged(callback: () => void): () => void {
  const unlisten = listen("library-changed", callback)
  return () => void unlisten.then((stop) => stop())
}
