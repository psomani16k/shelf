import { IS_TAURI } from "@/lib/platform"
import { httpBackend } from "./http"
import { tauriBackend } from "./tauri"
import type { Backend } from "./types"

export { ApiError } from "./error"

/** The one backend this build talks to: Tauri commands in the app, HTTP in a browser. */
const api: Backend = IS_TAURI ? tauriBackend : httpBackend

export const {
  listBooks,
  listDirectories,
  getProgress,
  setProgress,
  getState,
  setState,
  getConfig,
  setConfig,
  listThemes,
  setTheme,
  deleteTheme,
  renameTheme,
  exportUserData,
  importUserData,
  rebuildIndex,
  cleanUpProgress,
  getCover,
  getBookFile,
  uploadBook,
  createDirectory,
  deleteItems,
  login,
  logout,
  me,
  changePassword,
  listSessions,
  signOutSession,
  signUp,
  listUserRequests,
  approveUserRequest,
  denyUserRequest,
  listUsers,
  setUserRole,
  setUserScope,
  listScopeDirectories,
} = api
