import { isTauri } from "@tauri-apps/api/core"

/** True inside the Tauri app, false in a browser. Decided once, at load. */
export const IS_TAURI = isTauri()

export const IS_ANDROID = IS_TAURI && /android/i.test(navigator.userAgent)
