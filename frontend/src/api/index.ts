import { IS_TAURI } from "@/lib/platform"
import { httpBackend } from "./http"
import { tauriBackend } from "./tauri"
import type { Backend } from "./types"

export { ApiError } from "./error"
export type { Backend } from "./types"

/** The one backend this build talks to: Tauri commands in the app, HTTP in a browser. */
export const api: Backend = IS_TAURI ? tauriBackend : httpBackend

export const {
  listBooks,
  listDirectories,
  getProgress,
  setProgress,
  getState,
  setState,
  getConfig,
  setConfig,
  getCover,
  getBookFile,
} = api
