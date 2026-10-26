import { listen } from "@tauri-apps/api/event"
import { invokeCommand } from "./tauri"

/** App only: the libraries this device knows about, kept in the app's private storage. */

export type LibraryInfo = {
  name: string
  kind: "local" | "remote"
  /** The folder for a local library, the server URL for a remote one. */
  location: string
}

export type Libraries = {
  /** The library the app opens; unset before the first one is added or after it's removed. */
  current: string | null
  libraries: LibraryInfo[]
}

export const listLibraries = () => invokeCommand<Libraries>("list_libraries")

/** Names are required and unique (ignoring case); a bad one rejects with a 400. */
export const addLocalLibrary = (name: string, path: string) =>
  invokeCommand<LibraryInfo>("add_local_library", { name, path })

/** `baseUrl` is the server's origin, e.g. `http://nas:8080`; `http://` is assumed when left out. */
export const addRemoteLibrary = (name: string, baseUrl: string) =>
  invokeCommand<LibraryInfo>("add_remote_library", { name, baseUrl })

/** Makes it the library the app opens, now and on the next start. */
export const selectLibrary = (name: string) => invokeCommand<void>("select_library", { name })

/** Forgets the library on this device; nothing in its folder is deleted. */
export const removeLibrary = (name: string) => invokeCommand<void>("remove_library", { name })

/** Resolves once the library is ready to browse; a local one syncs its `.shelf/` first. */
export const openLibrary = (name: string) => invokeCommand<void>("open_library", { name })

/** Android: whether the app may read the device's files. Always true elsewhere. */
export const storageGranted = () => invokeCommand<boolean>("storage_granted")

/** Android: opens the permission prompt or settings page; resolves with the outcome on return. */
export const requestStorage = () => invokeCommand<boolean>("request_storage")

/** Android's folder picker. Rejects when cancelled. */
export const pickFolder = () => invokeCommand<string>("pick_folder")

export type Insets = { top: number; right: number; bottom: number; left: number }

/** Space taken by the status bar, navigation bar and cutout, in CSS px. Zero on desktops. */
export const systemInsets = () => invokeCommand<Insets>("system_insets")

/** The watcher synced a book. Returns the unsubscribe function. */
export function onLibraryChanged(callback: () => void): () => void {
  const unlisten = listen("library-changed", callback)
  return () => void unlisten.then((stop) => stop())
}
