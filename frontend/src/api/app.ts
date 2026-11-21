import { listen } from "@tauri-apps/api/event"
import { invokeCommand } from "./tauri"

export type LibraryInfo = {
  name: string
  kind: "local" | "remote"
  /** The folder for a local library, the server URL for a remote one. */
  location: string
  /** Whether a remote library has a login key, or a local one has a chosen user. */
  signed_in: boolean
  /** Local only: who reads it on this device, once chosen. */
  user?: string
}

export type Libraries = {
  /** The library the app opens; unset before the first one is added or after it's removed. */
  current: string | null
  libraries: LibraryInfo[]
}

export const listLibraries = () => invokeCommand<Libraries>("list_libraries")

/** The users with data in the open local library, sorted, to choose who's reading. */
export const listLocalUsers = () => invokeCommand<string[]>("list_local_users")

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

type Insets = { top: number; right: number; bottom: number; left: number }

/** Space taken by the status bar, navigation bar and cutout, in CSS px. Zero on desktops. */
export const systemInsets = () => invokeCommand<Insets>("system_insets")

/** Whether Android's floating Copy/Share bar shows for selected text. Does nothing elsewhere. */
export const setSelectionMenu = (shown: boolean) =>
  invokeCommand<void>("set_selection_menu", { shown })

export const copyText = (text: string) => invokeCommand<void>("copy_text", { text })

/** The interface's size on this device, in percent. */
export const uiScale = () => invokeCommand<number>("ui_scale")

/** 50 to 200 in steps of 10, else a 400. Resolves with the saved value. */
export const setUiScale = (percent: number) => invokeCommand<number>("set_ui_scale", { percent })

type LogSettings = {
  /** How many log files are kept, the one being written included. */
  keep_files: number
  max_keep_files: number
}

export const logSettings = () => invokeCommand<LogSettings>("log_settings")

/** 1 to `max_keep_files`, else a 400. Files past the new count are deleted at once. */
export const setLogFiles = (keepFiles: number) =>
  invokeCommand<LogSettings>("set_log_files", { keepFiles })

export type LogFileInfo = {
  /** `shelf-<started>.log`, one per app start. */
  name: string
  bytes: number
  /** RFC 3339 time the app started this file. */
  started: string
  /** RFC 3339 time of the last write. */
  last_write: string | null
  /** The file this run is writing to. */
  current: boolean
}

type LogFileList = {
  /** Where the files are. Only meaningful on a desktop; Android's is private to the app. */
  dir: string
  /** Newest first. */
  files: LogFileInfo[]
}

export const listLogFiles = () => invokeCommand<LogFileList>("list_log_files")

/** Puts the most recent log lines on the clipboard. */
export const copyLogs = () => invokeCommand<void>("copy_logs")

/** Asks where to save, then writes every log file into one. `false` if cancelled. */
export const saveLogs = () => invokeCommand<boolean>("save_logs")

/** Asks where to save, then copies that one log file there. `false` if cancelled. */
export const saveLogFile = (name: string) => invokeCommand<boolean>("save_log_file", { name })

/** Asks where to save, then writes `contents` there as `name`. `false` if cancelled. */
export const saveJsonFile = (name: string, contents: string) =>
  invokeCommand<boolean>("save_json_file", { name, contents })

/** The watcher synced a book. Returns the unsubscribe function. */
export function onLibraryChanged(callback: () => void): () => void {
  const unlisten = listen("library-changed", callback)
  return () => void unlisten.then((stop) => stop())
}

/** Android and Linux: keeps the screen from dimming, sleeping or locking while `on`. */
export const setKeepScreenOn = (on: boolean) => invokeCommand<void>("set_keep_screen_on", { on })
